// Arrow batch interchange for scalar UDF results. No host query execution.
struct HostArrowRelation {
    ArrowArrayStreamWrapper stream;
    ArrowTableSchema table;
    mutex lock;
    bool claimed = false;
};
struct HostRelationData : FunctionData {
    shared_ptr<HostArrowRelation> source;
    unique_ptr<FunctionData> Copy() const override {
        auto copy = make_uniq<HostRelationData>();
        copy->source = source;
        return std::move(copy);
    }
    bool Equals(const FunctionData &other) const override {
        return source == other.Cast<HostRelationData>().source;
    }
};
// Preserve authoritative language diagnostics across nested Arrow/Rust error
// envelopes. Never infer a diagnosis from human-readable error text.
unordered_map<string,string> HostErrorMetadata(string message) {
    unordered_map<string,string> metadata;
    for(idx_t depth=0;depth<16;depth++) {
        auto start=message.find('{');
        if(start==string::npos) {break;}
        auto detail=Json::parse(message.substr(start),nullptr,false);
        if(detail.is_discarded() || !detail.is_object()) {break;}
        for(auto key:{"orchid_classification","orchid_diagnosis"}) {
            if(detail.contains(key) && !detail.at(key).is_null()) {
                metadata[key]=detail.at(key).is_string() ? detail.at(key).get<string>() : detail.at(key).dump();
            }
        }
        if(!metadata.empty()) {break;}
        if(detail.contains("exception_message") && detail.at("exception_message").is_string()) {message=detail.at("exception_message").get<string>();}
        else if(detail.contains("error") && detail.at("error").is_string()) {message=detail.at("error").get<string>();}
        else {break;}
    }
    return metadata;
}
void CheckHostArrow(ArrowArrayStream &stream, int code) {
    if (!code) { return; }
    auto error = stream.get_last_error ? stream.get_last_error(&stream) : nullptr;
    auto message=string(error ? error : "unknown error");
    throw InvalidInputException(HostErrorMetadata(message),"Host Arrow stream: "+message);
}
shared_ptr<HostRelationData> ImportHostRelation(ClientContext &context, ArrowArrayStream &stream) {
    if (!stream.release || !stream.get_schema || !stream.get_next) {
        throw InvalidInputException("Released or invalid host Arrow stream");
    }
    auto relation = make_shared_ptr<HostRelationData>();
    relation->source = make_shared_ptr<HostArrowRelation>();
    auto &owned = relation->source->stream.arrow_array_stream;
    owned = stream;
    stream.release = nullptr;
    ArrowSchemaWrapper schema;
    CheckHostArrow(owned, owned.get_schema(&owned, &schema.arrow_schema));
    ArrowTableFunction::PopulateArrowTableSchema(context, relation->source->table, schema.arrow_schema);
    return relation;
}
struct HostRelationCursor {
    shared_ptr<HostArrowRelation> source;
    unique_ptr<ArrowScanLocalState> scan;
    bool done = false;
    explicit HostRelationCursor(shared_ptr<HostArrowRelation> source) : source(std::move(source)) {
        lock_guard<mutex> guard(this->source->lock);
        if (this->source->claimed) {
            throw InvalidInputException("Host Arrow input is single-pass; a repeated scan requires an explicitly materialized CTE");
        }
        this->source->claimed = true;
    }
    bool Next(ClientContext &context, DataChunk &output) {
        if (done) { return false; }
        if (context.IsInterrupted()) { throw InterruptException(); }
        auto &stream = source->stream.arrow_array_stream;
        while (!scan || scan->chunk_offset >= NumericCast<idx_t>(scan->chunk->arrow_array.length)) {
            scan.reset();
            auto array = make_uniq<ArrowArrayWrapper>();
            CheckHostArrow(stream, stream.get_next(&stream, &array->arrow_array));
            if (context.IsInterrupted()) { throw InterruptException(); }
            if (!array->arrow_array.release) { done = true; return false; }
            if (array->arrow_array.length < 0) { throw InvalidInputException("Negative Arrow batch length"); }
            if (array->arrow_array.length == 0) { continue; }
            scan = make_uniq<ArrowScanLocalState>(std::move(array), context);
        }
        output.Reset();
        output.SetCardinality(MinValue<idx_t>(STANDARD_VECTOR_SIZE,
            NumericCast<idx_t>(scan->chunk->arrow_array.length) - scan->chunk_offset));
        ArrowTableFunction::ArrowToDuckDB(*scan, source->table.GetColumns(), output);
        scan->chunk_offset += output.size();
        return true;
    }
};
