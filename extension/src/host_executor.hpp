// Bounded results from nested DuckDB SQL stages. The stream owns the prepared
// plan and executor; no background task can outlive their owning query result.
struct HostProfilerScope {
    ClientData &client;
    shared_ptr<QueryProfiler> previous;
    HostProfilerScope(ClientContext &context, shared_ptr<QueryProfiler> profiler)
        : client(ClientData::Get(context)), previous(client.profiler) { client.profiler = std::move(profiler); }
    ~HostProfilerScope() { client.profiler = std::move(previous); }
};
struct HostResultBuffer {
    mutex lock;
    std::deque<unique_ptr<DataChunk>> chunks;
    vector<InterruptState> blocked;
    bool HasChunk() {lock_guard<mutex> guard(lock);return !chunks.empty();}
    unique_ptr<DataChunk> Pop() {
        unique_ptr<DataChunk> result;
        vector<InterruptState> wake;
        {
            lock_guard<mutex> guard(lock);
            if (chunks.empty()) { return nullptr; }
            result = std::move(chunks.front());
            chunks.pop_front();
            wake.swap(blocked);
        }
        for (auto &interrupt : wake) { interrupt.Callback(); }
        return result;
    }
};
struct HostCollectorLocalState : LocalSinkState {bool published=false;};
class PhysicalHostCollector : public PhysicalResultCollector {
public:
    shared_ptr<HostResultBuffer> buffer;
    PhysicalHostCollector(PhysicalPlan &plan, PreparedStatementData &prepared, shared_ptr<HostResultBuffer> buffer)
        : PhysicalResultCollector(plan, prepared), buffer(std::move(buffer)) {}
    bool IsStreaming() const override { return true; }
    bool ParallelSink() const override { return false; }
    bool SinkOrderDependent() const override { return true; }
    unique_ptr<GlobalSinkState> GetGlobalSinkState(ClientContext &) const override { return make_uniq<GlobalSinkState>(); }
    unique_ptr<LocalSinkState> GetLocalSinkState(ExecutionContext &) const override { return make_uniq<HostCollectorLocalState>(); }
    SinkResultType Sink(ExecutionContext &context, DataChunk &chunk, OperatorSinkInput &input) const override {
        auto &local=input.local_state.Cast<HostCollectorLocalState>();
        lock_guard<mutex> guard(buffer->lock);
        if(local.published) {
            // DuckDB retries the same input after BLOCKED. It was already
            // copied to the consumer, so acknowledge without duplicating it.
            local.published=false;
            return SinkResultType::NEED_MORE_INPUT;
        }
        if(!buffer->chunks.empty()) {
            buffer->blocked.push_back(input.interrupt_state);
            return SinkResultType::BLOCKED;
        }
        auto copy=make_uniq<DataChunk>();
        copy->Initialize(Allocator::Get(context.client),chunk.GetTypes());
        chunk.Copy(*copy);
        buffer->chunks.push_back(std::move(copy));
        local.published=true;
        buffer->blocked.push_back(input.interrupt_state);
        // Yield after publishing, before asking a remote source for another
        // batch. This guarantees first-row latency even when its next page stalls.
        return SinkResultType::BLOCKED;
    }

    SinkCombineResultType Combine(ExecutionContext &, OperatorSinkCombineInput &) const override {
        return SinkCombineResultType::FINISHED;
    }
    unique_ptr<QueryResult> GetResult(GlobalSinkState &) const override {
        throw InternalException("Host SQL results must be pulled from their owning executor");
    }
};
class HostStreamResult : public QueryResult {
public:
    shared_ptr<ClientContext> context;
    unique_ptr<PreparedStatementData> prepared;
    shared_ptr<QueryProfiler> profiler;
    shared_ptr<HostResultBuffer> buffer = make_shared_ptr<HostResultBuffer>();
    unique_ptr<Executor> executor;
    bool finished = false;
    HostStreamResult(ClientContext &context, unique_ptr<PreparedStatementData> prepared,
                     shared_ptr<QueryProfiler> profiler)
        : QueryResult(QueryResultType::ARROW_RESULT, prepared->statement_type, prepared->properties,
                      prepared->types, prepared->names, context.GetClientProperties()),
          context(context.shared_from_this()), prepared(std::move(prepared)), profiler(std::move(profiler)) {}
    ~HostStreamResult() override {
        if (executor) {
            HostProfilerScope scope(*context, profiler);
            executor->CancelTasks();
            executor.reset();
        }
    }
    string ToString() override { return "Orchid streaming SQL result"; }
protected:
    unique_ptr<DataChunk> FetchInternal() override {
        HostProfilerScope scope(*context, profiler);
        if (!executor && !finished) {
            executor = make_uniq<Executor>(*context);
            executor->Initialize(make_uniq<PhysicalHostCollector>(*prepared->physical_plan, *prepared, buffer));
        }
        for (;;) {
            // DuckDB marks the context interrupted after an operator failure;
            // report that original producer error before the generic flag.
            if (executor && executor->HasError()) { executor->ThrowException(); }
            if (context->IsInterrupted()) { throw InterruptException(); }
            auto chunk = buffer->Pop();
            if (chunk) { return chunk; }
            if (finished) { return nullptr; }
            auto status = executor->ExecuteTask();
            if (status == PendingExecutionResult::EXECUTION_ERROR) { executor->ThrowException(); }
            if (status == PendingExecutionResult::EXECUTION_FINISHED) {
                finished = true;
                executor->CancelTasks();
            } else if (status == PendingExecutionResult::BLOCKED || status == PendingExecutionResult::NO_TASKS_AVAILABLE) {
                if(!buffer->HasChunk()) {executor->WaitForTask();}
            }
        }
    }
};
unique_ptr<QueryResult> HostExecuteStream(ClientContext &context, unique_ptr<SQLStatement> statement,
                                        const vector<Value> &params) {
    // Effects must finish before subsequent program stages run, even if the
    // caller never consumes returned rows. Only read-only SELECT stages stream.
    if (statement->type != StatementType::SELECT_STATEMENT) {
        return HostExecute(context, std::move(statement), params);
    }
    ValidateProtectedFunctions(context, statement->Cast<SelectStatement>());
    auto profiler = make_shared_ptr<QueryProfiler>(context);
    HostProfilerScope scope(context, profiler);
    Planner planner(context);
    for (idx_t i = 0; i < params.size(); i++) {
        planner.parameter_data[std::to_string(i + 1)] = BoundParameterData(params[i]);
    }
    auto effect_statement = statement->Copy();
    planner.CreatePlan(std::move(statement));
    if (!planner.properties.modified_databases.empty()) {
        // Sequences and other modifying SELECT functions are effect fences.
        return HostExecute(context, std::move(effect_statement), params);
    }
    Optimizer optimizer(*planner.binder, context);
    auto logical = optimizer.Optimize(std::move(planner.plan));
    auto prepared = make_uniq<PreparedStatementData>(StatementType::SELECT_STATEMENT);
    prepared->names = planner.names;
    prepared->types = planner.types;
    prepared->properties = planner.properties;
    prepared->output_type = QueryResultOutputType::ALLOW_STREAMING;
    prepared->memory_type = QueryResultMemoryType::BUFFER_MANAGED;
    PhysicalPlanGenerator generator(context);
    prepared->physical_plan = generator.Plan(std::move(logical));
    return make_uniq<HostStreamResult>(context, std::move(prepared), std::move(profiler));
}
