#pragma once
#include "arrow_abi.h"
#include <nlohmann/json.hpp>
#include <cstdlib>
#include <functional>
#include <fstream>
#include <memory>
#include <map>
#include <exception>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>
#ifdef _WIN32
#include <windows.h>
#else
#include <dlfcn.h>
#endif

namespace orchiddb {
using Json = nlohmann::json;
struct CompiledQuery { std::string dialect, sql; std::vector<std::string> fields; Json diagnostics = Json::object(); };

/** Build provider-neutral permission input records for a compiler request. */
inline Json permission_relation(std::string table, std::string resource_type,
    std::string permission, Json columns = Json::object()) {
  Json relation = {
    {"table", std::move(table)}, {"resource_type", std::move(resource_type)},
    {"permission", std::move(permission)}, {"resource_type_column", "resource_type"},
    {"permission_column", "resource_rel"}, {"resource_id_column", "resource_id"},
    {"subject_type_column", "subject_type"}, {"subject_relation_column", "subject_rel"},
    {"subject_id_column", "subject_id"}
  };
  if (!columns.is_object()) throw std::invalid_argument("Permission relation columns must be an object");
  for (auto it = columns.begin(); it != columns.end(); ++it) relation[it.key()] = it.value();
  return relation;
}
inline Json permission_scope(std::string resource_column, Json relation) {
  return {{"resource_column", std::move(resource_column)}, {"relation", std::move(relation)}};
}
inline Json authorization(std::string subject_type, std::string subject_id) {
  return {{"subject_type", std::move(subject_type)}, {"subject_id", std::move(subject_id)}};
}

/** Shared compiler and explicit statistics coordinator. Connections remain application-owned. */
class Compiler {
  struct Library {
#ifdef _WIN32
    HMODULE handle;
    explicit Library(const char* path) : handle(LoadLibraryA(path)) { if (!handle) throw std::runtime_error("Cannot load OrchidDB compiler"); }
    // Native code owns a process-wide runtime; unloading underneath its worker
    // threads is unsafe. Session handles are released separately.
    ~Library() = default;
    void* symbol(const char* name) { auto p = reinterpret_cast<void*>(GetProcAddress(handle, name)); if (!p) throw std::runtime_error(name); return p; }
#else
    void* handle;
    explicit Library(const char* path) : handle(dlopen(path, RTLD_NOW | RTLD_LOCAL)) { if (!handle) throw std::runtime_error(dlerror()); }
    // Match the other native bindings: keep runtime code mapped until exit.
    ~Library() = default;
    void* symbol(const char* name) { auto p = dlsym(handle, name); if (!p) throw std::runtime_error(name); return p; }
#endif
  };
  std::shared_ptr<Library> library_;
  char* (*compile_)(const char*);
  char* (*bind_arrow_)(const char*, ArrowArrayStream*);
  char* (*statistics_)(const char*);
  void (*free_)(char*);
  const char* (*revision_)();
public:
  static std::string default_path() {
    if (auto value = std::getenv("ORCHIDDB_NATIVE_LIBRARY")) return value;
#ifdef _WIN32
    return "orchiddb_compiler.dll";
#elif defined(__APPLE__)
    return "liborchiddb_compiler.dylib";
#else
    return "liborchiddb_compiler.so";
#endif
  }
  explicit Compiler(const std::string& path = default_path()) : library_(std::make_shared<Library>(path.c_str())) {
    auto abi = reinterpret_cast<uint32_t(*)()>(library_->symbol("orchiddb_abi_version"));
    if (abi() != 1) throw std::runtime_error("Unsupported OrchidDB ABI");
    compile_ = reinterpret_cast<char*(*)(const char*)>(library_->symbol("orchiddb_compile_json"));
    bind_arrow_ = reinterpret_cast<char*(*)(const char*, ArrowArrayStream*)>(library_->symbol("orchiddb_bind_arrow_json"));
    statistics_ = reinterpret_cast<char*(*)(const char*)>(library_->symbol("orchiddb_statistics_json"));
    free_ = reinterpret_cast<void(*)(char*)>(library_->symbol("orchiddb_string_free"));
    revision_ = reinterpret_cast<const char*(*)()>(library_->symbol("orchiddb_core_revision"));
  }
  static CompiledQuery from_plan(const Json& plan) {
    return {plan.at("dialect"), plan.at("sql"), plan.at("fields").get<std::vector<std::string>>(), plan};
  }
  Json command(const Json& command) const {
    const auto encoded = command.dump();
    std::unique_ptr<char, void(*)(char*)> response(compile_(encoded.c_str()), free_);
    if (!response) throw std::runtime_error("Null compiler response");
    const auto envelope = Json::parse(response.get());
    if (!envelope.value("ok", false)) throw std::runtime_error(envelope.value("error", "Compiler error"));
    return envelope.at("result");
  }
  Json remote_command(const Json& command) const {
    auto remote = reinterpret_cast<char*(*)(const char*)>(library_->symbol("orchiddb_remote_json"));
    const auto encoded = command.dump();
    std::unique_ptr<char, void(*)(char*)> response(remote(encoded.c_str()), free_);
    if (!response) throw std::runtime_error("Null remote engine response");
    const auto envelope = Json::parse(response.get());
    if (!envelope.value("ok", false)) throw std::runtime_error(envelope.value("error", "Remote engine error"));
    return envelope.at("result");
  }
  Json bind_arrow_command(const Json& command, ArrowArrayStream* stream) const {
    const auto encoded = command.dump();
    std::unique_ptr<char,void(*)(char*)> response(bind_arrow_(encoded.c_str(),stream),free_);
    if (!response) throw std::runtime_error("Null Arrow binding response");
    const auto envelope=Json::parse(response.get());
    if (!envelope.value("ok",false)) throw std::runtime_error(envelope.value("error","Arrow binding failed"));
    return envelope.at("result");
  }
  CompiledQuery bind_arrow(const CompiledQuery& plan, const std::string& relation, ArrowArrayStream* stream) const {
    return from_plan(bind_arrow_command({{"op","bind"},{"plan",plan.diagnostics},{"relation",relation}},stream));
  }
  std::string core_revision() const { return revision_(); }
  Json statistics_command(const Json& command) const {
    const auto encoded = command.dump();
    std::unique_ptr<char, void(*)(char*)> response(statistics_(encoded.c_str()), free_);
    if (!response) throw std::runtime_error("Null statistics response");
    auto envelope = Json::parse(response.get());
    if (!envelope.value("ok", false)) throw std::runtime_error(envelope.value("error", "Statistics error"));
    return envelope.at("result");
  }
  CompiledQuery compile(const Json& request, const Json& catalog_id = nullptr) const {
    Json plan;
    if (!catalog_id.is_null()) {
      plan = statistics_command({{"op", "compile"}, {"catalog_id", catalog_id}, {"request", request}});
    } else {
      const auto encoded = request.dump();
      std::unique_ptr<char, void(*)(char*)> response(compile_(encoded.c_str()), free_);
      if (!response) throw std::runtime_error("Null compiler response");
      auto envelope = Json::parse(response.get());
      if (!envelope.value("ok", false)) throw std::runtime_error(envelope.value("error", "Compiler error"));
      plan = envelope.at("result");
    }
    if (plan.at("version") != 1 || plan.at("dialect") != request.at("dialect")) throw std::runtime_error("Invalid compiler response");
    return {plan.at("dialect").get<std::string>(), plan.at("sql").get<std::string>(), plan.at("fields").get<std::vector<std::string>>(), plan};
  }
};

/** Shared coordinator catalog; collection callback borrows an application-owned session.
 * Callback must honor timeout_ms/max_rows/max_bytes, returning rows or base64 IPC.
 * Throw if the session cannot execute with those bounds. No client estimators.
 */
class Statistics {
  Compiler compiler_;
  Json catalog_id_, snapshot_, report_;
public:
  explicit Statistics(Compiler compiler) : compiler_(std::move(compiler)) {}
  Statistics(const Statistics&) = delete;
  Statistics& operator=(const Statistics&) = delete;
  ~Statistics() { try { clear(); } catch (...) {} }
  const Json& snapshot() const { return snapshot_; }
  const Json& report() const { return report_; }
  void clear() {
    if (!catalog_id_.is_null()) compiler_.statistics_command({{"op", "release"}, {"catalog_id", catalog_id_}});
    catalog_id_ = snapshot_ = report_ = nullptr;
  }
  void install(const Json& snapshot) {
    auto result = compiler_.statistics_command({{"op", "install"}, {"snapshot", snapshot}});
    clear(); catalog_id_ = result.at("catalog_id"); snapshot_ = snapshot;
  }
  void save(const std::string& path) const {
    if (snapshot_.is_null()) throw std::runtime_error("No statistics installed");
    std::ofstream output(path);
    output << snapshot_.dump();
    if (!output) throw std::runtime_error("Cannot save statistics snapshot");
  }
  void load(const std::string& path) {
    std::ifstream input(path);
    if (!input) throw std::runtime_error("Cannot load statistics snapshot");
    install(Json::parse(input));
  }
  void generate(const Json& request, const std::function<Json(const Json&)>& collect) {
    auto state = compiler_.statistics_command({{"op", "begin"}, {"request", request}});
    const auto id = state.at("id");
    try {
      while (!state.at("request").is_null()) {
        const auto work = state.at("request");
        Json response;
        try { response = collect(work); }
        catch (const std::exception& e) { response = {{"error", e.what()}}; }
        response["op"] = "submit"; response["id"] = id; response["request_id"] = work.at("id");
        state = compiler_.statistics_command(response);
      }
      auto result = compiler_.statistics_command({{"op", "finish"}, {"id", id}});
      clear(); catalog_id_ = result.at("catalog_id"); snapshot_ = result.at("snapshot"); report_ = result.at("report");
    } catch (...) {
      try { compiler_.statistics_command({{"op", "cancel"}, {"id", id}}); } catch (...) {}
      throw;
    }
  }
  CompiledQuery compile(const Json& request) const { return compiler_.compile(request, catalog_id_); }
};

/** C Data objects own their release callback independently of the stream. */
template<class T> class ArrowOwner {
  T value_{};
public:
  ArrowOwner() = default;
  explicit ArrowOwner(T value) : value_(value) {}
  ArrowOwner(const ArrowOwner&) = delete;
  ArrowOwner& operator=(const ArrowOwner&) = delete;
  ArrowOwner(ArrowOwner&& other) noexcept : value_(other.value_) { other.value_ = {}; }
  ArrowOwner& operator=(ArrowOwner&& other) noexcept { if (this != &other) { close(); value_ = other.value_; other.value_ = {}; } return *this; }
  ~ArrowOwner() { close(); }
  void close() noexcept { if (value_.release) value_.release(&value_); value_ = {}; }
  T* get() { return &value_; }
  const T* get() const { return &value_; }
  explicit operator bool() const { return value_.release != nullptr; }
};
using RecordBatch = ArrowOwner<ArrowArray>;
using Schema = ArrowOwner<ArrowSchema>;

/** Consumes an ArrowArrayStream: callbacks and ownership move into this object. */
class ArrowResult {
  ArrowOwner<ArrowArrayStream> stream_;
  void check(int code) {
    if (code) { const char* message = stream_.get()->get_last_error(stream_.get()); throw std::runtime_error(message ? message : "Arrow stream failed"); }
  }
  void require_open() { if (!stream_) throw std::runtime_error("Arrow result is closed"); }
public:
  explicit ArrowResult(ArrowArrayStream& stream) : stream_(stream) {
    stream = {};
    if (!stream_ || !stream_.get()->get_schema || !stream_.get()->get_next || !stream_.get()->get_last_error)
      throw std::invalid_argument("Invalid Arrow C stream");
  }
  ArrowResult(ArrowResult&&) = default;
  ArrowResult& operator=(ArrowResult&&) = default;
  Schema schema() { require_open(); Schema out; check(stream_.get()->get_schema(stream_.get(), out.get())); return out; }
  /** A batch with no release callback marks EOF. Prior batches retain independent ownership. */
  RecordBatch next() { require_open(); RecordBatch out; check(stream_.get()->get_next(stream_.get(), out.get())); return out; }
  ArrowArrayStream* stream_for_binding() { require_open(); return stream_.get(); }
  void close() noexcept { stream_.close(); }
};

/** Engine is borrowed; the returned stream owns only its result resources. */
class ExecutionEngine {
public:
  virtual ~ExecutionEngine() = default;
  virtual std::string id() const = 0;
  virtual std::string dialect() const = 0;
  virtual ArrowResult execute(const CompiledQuery&) { throw std::runtime_error("SQL execution unsupported"); }
  /** Return {"rows": typed cell arrays} or {"ipc": base64 Arrow stream}. */
  virtual Json execute_requests(const Json&, const Json&) { throw std::runtime_error("Prepared request execution unsupported"); }
  // Override using the existing session when it can enforce all supplied bounds.
  virtual Json collect_statistics(const Json&) { throw std::runtime_error("Bounded statistics execution unsupported"); }
};
/** Optional HTTP engine backed by the shared native transport. Closing the engine
 * never closes caller-owned SQL sessions. Each instance owns one native handle. */
class RemoteEngine final : public ExecutionEngine {
  Compiler compiler_;
  std::string adapter_;
  Json handle_;
public:
  RemoteEngine(Compiler compiler, std::string adapter, Json options)
      : compiler_(std::move(compiler)), adapter_(std::move(adapter)) {
    const auto opened=compiler_.remote_command({{"op","open"},{"adapter",adapter_},{"options",std::move(options)}});
    handle_=opened.at("id");
  }
  RemoteEngine(const RemoteEngine&)=delete;
  RemoteEngine& operator=(const RemoteEngine&)=delete;
  RemoteEngine(RemoteEngine&& other) noexcept
      : compiler_(std::move(other.compiler_)), adapter_(std::move(other.adapter_)), handle_(std::move(other.handle_)) { other.handle_=nullptr; }
  ~RemoteEngine() { try { close(); } catch (...) {} }
  std::string id() const override { return adapter_; }
  std::string dialect() const override { return adapter_; }
  void close() {
    if (!handle_.is_null()) {
      compiler_.remote_command({{"op","close"},{"id",handle_}});
      handle_=nullptr;
    }
  }
  void clear_metadata_cache() {
    require_open(); compiler_.remote_command({{"op","clear_metadata_cache"},{"id",handle_}});
  }
  Json execute_requests(const Json& requests,const Json& columns) override {
    require_open();
    return compiler_.remote_command({{"op","execute"},{"id",handle_},{"requests",requests},{"columns",columns},{"format","ipc"}});
  }
private:
  void require_open() const { if(handle_.is_null()) throw std::runtime_error("Remote engine is closed"); }
};

namespace detail {
inline bool same_schema(const ArrowSchema* a,const ArrowSchema* b) {
  if (!a || !b || std::string(a->format?a->format:"")!=std::string(b->format?b->format:"") || std::string(a->name?a->name:"")!=std::string(b->name?b->name:"") || a->flags!=b->flags || a->n_children!=b->n_children || bool(a->dictionary)!=bool(b->dictionary)) return false;
  for(int64_t i=0;i<a->n_children;++i) if(!same_schema(a->children[i],b->children[i])) return false;
  return !a->dictionary || same_schema(a->dictionary,b->dictionary);
}
/** Concatenate operation results without converting typed Arrow values. */
inline ArrowResult concatenate(std::vector<ArrowResult> results) {
  if(results.empty()) throw std::invalid_argument("Cannot concatenate no streams");
  auto schema=results.front().schema();
  for(size_t i=1;i<results.size();++i) {
    auto next_schema=results[i].schema();
    if(!same_schema(schema.get(),next_schema.get())) throw std::runtime_error("Dependent operation result schema mismatch");
  }
  struct State { std::vector<ArrowResult> results; size_t current=0; std::string error; };
  auto state=std::make_unique<State>(); state->results=std::move(results);
  ArrowArrayStream stream{}; stream.private_data=state.release();
  stream.get_schema=[](ArrowArrayStream* s,ArrowSchema* out) {
    auto& state=*static_cast<State*>(s->private_data);
    auto* source=state.results.front().stream_for_binding();
    const auto code=source->get_schema(source,out);
    if(code) {const auto* error=source->get_last_error(source);state.error=error?error:"Arrow schema failed";}
    return code;
  };
  stream.get_next=[](ArrowArrayStream* s,ArrowArray* out) {
    auto& state=*static_cast<State*>(s->private_data);
    while(state.current<state.results.size()) {
      auto* source=state.results[state.current].stream_for_binding();
      const auto code=source->get_next(source,out);
      if(code) {const auto* error=source->get_last_error(source);state.error=error?error:"Arrow stream failed";return code;}
      if(out->release) return 0;
      // Keep every source open until release: get_schema remains valid after EOF.
      ++state.current;
    }
    *out={}; return 0;
  };
  stream.get_last_error=[](ArrowArrayStream* s) {return static_cast<State*>(s->private_data)->error.c_str();};
  stream.release=[](ArrowArrayStream* s) {delete static_cast<State*>(s->private_data);s->private_data=nullptr;s->release=nullptr;};
  return ArrowResult(stream);
}
inline CompiledQuery source_query(const Json& transfer,const Json& columns) {
  CompiledQuery query{transfer.at("source_dialect"),transfer.at("sql"),{}};
  query.diagnostics["field_types"]=Json::array();
  for(const auto& column:columns) {query.fields.push_back(column.at("name"));query.diagnostics["field_types"].push_back(column.at("data_type"));}
  return query;
}
} // namespace detail

inline ArrowResult query(const Compiler& compiler, const Json& request, ExecutionEngine& engine) {
  if (request.at("dialect") != engine.dialect()) throw std::invalid_argument("Compiler and engine SQL dialects differ");
  auto plan = compiler.compile(request);
  if (!plan.diagnostics.value("transfers", Json::array()).empty()) throw std::invalid_argument("Use query_federated for a multi-engine plan");
  return engine.execute(plan);
}
inline void generate_statistics(Statistics& statistics, const Json& request, ExecutionEngine& engine) {
  if (request.at("dialect") != engine.dialect()) throw std::invalid_argument("Compiler and engine SQL dialects differ");
  statistics.generate(request, [&](const Json& work) { return engine.collect_statistics(work); });
}
inline ArrowResult query(const Statistics& statistics, const Json& request, ExecutionEngine& engine) {
  if (request.at("dialect") != engine.dialect()) throw std::invalid_argument("Compiler and engine SQL dialects differ");
  auto plan = statistics.compile(request);
  if (!plan.diagnostics.value("transfers", Json::array()).empty()) throw std::invalid_argument("Use query_federated for a multi-engine plan");
  return engine.execute(plan);
}

/** Consume the result inside the callback. Sessions remain application-owned. */
inline void execute_federated(const Compiler& compiler, CompiledQuery plan,
    const std::map<std::string, ExecutionEngine*>& engines,
    const std::function<void(ArrowResult&)>& consume) {
  const auto target_id = plan.diagnostics.at("execution_engine").get<std::string>();
  const auto transfers = plan.diagnostics.value("transfers", Json::array());
  auto check = [&](const std::string& id, const std::string& dialect) {
    auto found = engines.find(id);
    if (found == engines.end() || !found->second || found->second->dialect() != dialect)
      throw std::invalid_argument("Missing engine or dialect mismatch: " + id);
  };
  check(target_id, plan.dialect);
  for (const auto& t : transfers) {
    check(t.at("source_engine"),t.at("source_dialect"));
    if(t.contains("request")) check(t.at("request").at("engine"),t.at("request").at("template").at("adapter"));
    if(t.contains("operation")) check(t.at("operation").at("engine"),t.at("operation").at("template").at("dialect"));
  }
  auto& target = *engines.at(target_id);
  while(!plan.diagnostics.value("transfers",Json::array()).empty()) {
    // Binding updates pending SQL, so always read from the current plan.
    const auto t=plan.diagnostics.at("transfers").front();
    const auto relation=t.at("target_relation");
    const Json operation=t.contains("request")?t.at("request"):t.value("operation",Json());
    if(operation.is_null()) {
      auto result=engines.at(t.at("source_engine"))->execute(detail::source_query(t,t.at("columns")));
      plan=compiler.bind_arrow(plan,relation,result.stream_for_binding());
      continue;
    }
    Json bound;
    const Json command={{"op","bind_operation"},{"plan",plan.diagnostics},{"relation",relation}};
    if(t.at("sql")=="") {
      auto closed=command;closed["rows"]=Json::array({Json::array()});bound=compiler.command(closed);
    } else {
      auto inputs=engines.at(t.at("source_engine"))->execute(detail::source_query(t,operation.at("input_columns")));
      bound=compiler.bind_arrow_command(command,inputs.stream_for_binding());
    }
    if(bound.contains("requests")) {
      auto data=engines.at(bound.at("engine"))->execute_requests(bound.at("requests"),bound.at("columns"));
      data["op"]="bind";data["plan"]=plan.diagnostics;data["relation"]=relation;
      plan=Compiler::from_plan(compiler.command(data));
    } else {
      std::vector<ArrowResult> outputs;
      for(const auto& statement:bound.at("sql")) {
        auto output=t;output["source_dialect"]=bound.at("dialect");output["sql"]=statement;
        outputs.push_back(engines.at(bound.at("engine"))->execute(detail::source_query(output,t.at("columns"))));
      }
      if(outputs.empty()) {
        plan=Compiler::from_plan(compiler.command({{"op","bind"},{"plan",plan.diagnostics},{"relation",relation},{"rows",Json::array()}}));
      } else {
        auto joined=detail::concatenate(std::move(outputs));
        plan=compiler.bind_arrow(plan,relation,joined.stream_for_binding());
      }
    }
  }
  auto result=target.execute(plan);
  consume(result);
}

inline void query_federated(const Compiler& compiler, const Json& request,
    const std::map<std::string, ExecutionEngine*>& engines,
    const std::function<void(ArrowResult&)>& consume) {
  execute_federated(compiler,compiler.compile(request),engines,consume);
}

} // namespace orchiddb
