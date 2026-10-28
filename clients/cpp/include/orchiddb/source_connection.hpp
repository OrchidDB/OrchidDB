#ifndef ORCHIDDB_SOURCE_CONNECTION_HPP
#define ORCHIDDB_SOURCE_CONNECTION_HPP
#include "orchiddb.hpp"
#include <cstdio>
#include <optional>

namespace orchiddb {
/** Positional rows from application-owned SQL callbacks or local functions.
 * The source implementation must release its cursor when closed or destroyed. */
class SourceRows {
public:
  virtual ~SourceRows() = default;
  virtual bool next(Json& row) = 0;
  virtual void close() = 0;
};
struct SqlSource {
  std::string dialect;
  std::function<std::shared_ptr<SourceRows>(const std::string&)> query;
  std::function<std::shared_ptr<SourceRows>(const Json&, const Json&)> requests;
  std::string concurrency_key;
  bool concurrent = false;
};
using LocalFunction = std::function<Json(const std::vector<Json>&)>;
using DuckDbCompletion = std::function<std::shared_ptr<SourceRows>(const std::string&, const std::map<std::string, std::shared_ptr<SourceRows>>&)>;
namespace detail {
struct CursorLease {
  std::shared_ptr<SourceRows> rows;
  ~CursorLease() { if(rows) { try { rows->close(); } catch(...) {} } }
};
class FunctionRows final : public SourceRows {
  std::function<bool(Json&)> next_;
public:
  explicit FunctionRows(std::function<bool(Json&)> next) : next_(std::move(next)) {}
  bool next(Json& row) override { return next_ && next_(row); }
  void close() override { next_ = {}; }
};
inline std::shared_ptr<SourceRows> replay_rows(std::shared_ptr<SourceRows> input) {
  // Anonymous disk spool preserves bounded memory and closes a non-reentrant
  // source cursor before any dependent operation opens another one.
  return std::make_shared<FunctionRows>([input=std::move(input), file=std::shared_ptr<std::FILE>{}](Json& row) mutable {
    if (!file) {
      file = std::shared_ptr<std::FILE>(std::tmpfile(), [](std::FILE* f){ if(f) std::fclose(f); });
      if (!file) throw std::runtime_error("Cannot open source replay file");
      Json value;
      try {
        while (input->next(value)) {
          const auto bytes=value.dump(); const auto size=static_cast<std::uint64_t>(bytes.size());
          if(std::fwrite(&size,sizeof(size),1,file.get())!=1 || std::fwrite(bytes.data(),1,bytes.size(),file.get())!=bytes.size()) throw std::runtime_error("Cannot write source replay file");
        }
        input->close(); input.reset(); std::rewind(file.get());
      } catch(...) { input->close(); throw; }
    }
    std::uint64_t size=0;
    if(std::fread(&size,sizeof(size),1,file.get())!=1) {if(std::feof(file.get()))return false;throw std::runtime_error("Cannot read replay file");}
    std::string bytes(size,'\0');
    if(std::fread(bytes.data(),1,bytes.size(),file.get())!=bytes.size())throw std::runtime_error("Truncated source replay row");
    row=Json::parse(bytes);return true;
  });
}
}

/** No bundled database drivers. The engine is only a completion capability. */
class SourceConnection {
  detail::Compiler runtime_;
  Json schema_;
  std::string engine_;
  std::map<std::string,SqlSource> sources_;
  std::map<std::string,LocalFunction> functions_;
  DuckDbCompletion completion_;
  bool closed_=false;
  static void validate_value(const Json& value,const std::string& type) {
    if(value.is_null())return;
    bool valid=true;
    if(type=="int64"||type=="int32")valid=value.is_number_integer();
    else if(type=="float64"||type=="float32")valid=value.is_number();
    else if(type=="boolean")valid=value.is_boolean();
    else if(type=="string")valid=value.is_string();
    else if(type.rfind("list<",0)==0&&type.back()=='>') {
      valid=value.is_array();if(valid)for(const auto& item:value)validate_value(item,type.substr(5,type.size()-6));
    }
    if(!valid)throw std::runtime_error("Local function returned an invalid value for "+type);
  }
  static Json evaluate(const Json& expr,const Json& row,const std::map<std::string,LocalFunction>& functions,const detail::Compiler& runtime) {
    const auto kind=expr.at("kind").get<std::string>();
    if(kind=="column")return row.at(expr.at("index").get<std::size_t>());
    if(kind=="literal")return expr.at("json");
    if(kind=="cast")return evaluate(expr.at("input"),row,functions,runtime);
    if(kind=="graph_element") {
      auto decoded=runtime.command({{"op","decode_graph_values"},{"data_type",expr.at("data_type")},{"values",Json::array({evaluate(expr.at("input"),row,functions,runtime)})}});
      if(!decoded.is_array()||decoded.size()!=1)throw std::runtime_error("Invalid graph decoder row count");
      return decoded.at(0);
    }
    if(kind=="call") {
      std::vector<Json> args; for(const auto& a:expr.at("arguments"))args.push_back(evaluate(a,row,functions,runtime));
      Json value=functions.at(expr.at("function").get<std::string>())(args);
      validate_value(value,expr.at("returns").get<std::string>());return value;
    }
    throw std::runtime_error("Unsupported local expression");
  }
  static Json bind_request(const Json& spec,const Json& row) {
    if(row.size()!=spec.at("parameters").get<std::size_t>())throw std::runtime_error("Request parameter count mismatch");
    Json request=spec.at("request");
    for(const auto& binding:spec.at("bindings")) {
      Json value=row.at(binding.at("parameter").get<std::size_t>());
      if(binding.value("encoding",std::string())=="sql_null")value=value.is_null();
      const auto pointer=Json::json_pointer(binding.at("pointer").get<std::string>());
      request.at(pointer)=std::move(value);
    }
    return request;
  }
  void check_expression(const Json& expr,std::size_t width) const {
    const auto kind=expr.at("kind").get<std::string>();
    if(kind=="column") {if(expr.at("index").get<std::size_t>()>=width)throw std::runtime_error("Column out of bounds");}
    else if(kind=="literal") {if(!expr.contains("json"))throw std::runtime_error("Literal requires an Arrow-capable adapter");}
    else if(kind=="cast"||kind=="graph_element")check_expression(expr.at("input"),width);
    else if(kind=="call") {
      const auto name=expr.at("function").get<std::string>();
      if(!functions_.count(name)||!functions_.at(name))throw std::runtime_error("Missing local function "+name);
      for(const auto& a:expr.at("arguments"))check_expression(a,width);
    } else throw std::runtime_error("Unsupported local expression");
  }
public:
  SourceConnection(Json schema,std::string engine,std::map<std::string,SqlSource> sources,
      std::map<std::string,LocalFunction> functions={},DuckDbCompletion completion={},
      const std::string& library=detail::Compiler::default_path())
    :runtime_(library),engine_(std::move(engine)),sources_(std::move(sources)),functions_(std::move(functions)),completion_(std::move(completion)) {
    if(engine_!="none"&&engine_!="duckdb")throw std::invalid_argument("Engine must be none or duckdb; register Trino as a source");
    schema_=runtime_.command({{"op","validate_schema"},{"schema",std::move(schema)}});
  }
  void close(){closed_=true;}
  std::shared_ptr<SourceRows> query(const std::string& text,Json parameters=Json::object(),const std::string& language="cypher",Json authorization=nullptr) {
    if(closed_)throw std::runtime_error("Connection closed");
    Json request=schema_;request["version"]=1;request["dialect"]="duckdb";request["engine"]=engine_;request["query"]=text;request["language"]=language;request["parameters"]=std::move(parameters);
    if(!authorization.is_null())request["authorization"]=std::move(authorization);
    const auto compiled=runtime_.command(request);
    if(compiled.at("version")!=2)throw std::runtime_error("Expected source-plan protocol");
    const auto plan=compiled.at("execution_plan"); const auto nodes=plan.at("nodes");
    const auto result=plan.at("result").get<std::size_t>();
    if(plan.at("version")!=1||result>=nodes.size())throw std::runtime_error("Invalid execution plan");
    for(std::size_t i=0;i<nodes.size();++i) {
      const auto& n=nodes[i];const auto kind=n.at("kind").get<std::string>();
      auto dependency=[i](std::size_t input){if(input>=i)throw std::runtime_error("Invalid plan dependency");};
      if(kind=="source_sql") {
        auto source=sources_.find(n.at("source").get<std::string>());
        if(source==sources_.end()||source->second.dialect!=n.at("dialect")||!source->second.query)throw std::runtime_error("Missing SQL source");
      } else if(kind=="source_request") {
        auto source=sources_.find(n.at("source").get<std::string>());
        if(source==sources_.end()||source->second.dialect!=n.at("template").at("adapter")||!source->second.requests)throw std::runtime_error("Missing request source");
        if(!n.at("input").is_null())dependency(n.at("input").get<std::size_t>());
      } else if(kind=="local_project") {
        const auto input=n.at("input").get<std::size_t>();dependency(input);
        for(const auto& expr:n.at("expressions"))check_expression(expr,nodes[input].at("columns").size());
      } else if(kind=="duckdb") {
        if(engine_!="duckdb"||!completion_)throw std::runtime_error("DuckDB completion callback required");
        for(const auto& r:n.at("inputs"))dependency(r.at("node").get<std::size_t>());
      } else if(kind!="input")throw std::runtime_error("Unsupported source-plan operation "+kind);
    }
    std::vector<std::shared_ptr<SourceRows>> outputs;
    for(std::size_t id=0;id<nodes.size();++id) {
      const auto n=nodes[id];const auto kind=n.at("kind").get<std::string>();std::shared_ptr<SourceRows> rows;
      if(kind=="source_sql") {
        const auto source=sources_.at(n.at("source").get<std::string>());
        rows=std::make_shared<detail::FunctionRows>([source,n,lease=std::make_shared<detail::CursorLease>()](Json& row)mutable{
          if(!lease->rows)lease->rows=source.query(n.at("sql").get<std::string>());if(!lease->rows)throw std::runtime_error("Source returned a null cursor");return lease->rows->next(row);
        });
      } else if(kind=="source_request") {
        const auto source=sources_.at(n.at("source").get<std::string>());
        const auto input=n.at("input").is_null()?std::shared_ptr<SourceRows>{}:outputs.at(n.at("input").get<std::size_t>());
        rows=std::make_shared<detail::FunctionRows>([source,input,n,once=false,lease=std::make_shared<detail::CursorLease>()](Json& row)mutable{
          for(;;) {
            if(lease->rows&&lease->rows->next(row))return true;
            if(lease->rows){lease->rows->close();lease->rows.reset();}
            Json requests=Json::array();
            if(!input){if(once)return false;once=true;requests.push_back(bind_request(n.at("template"),Json::array()));}
            else {Json value;for(std::size_t i=0;i<64&&input->next(value);++i)requests.push_back(bind_request(n.at("template"),value));if(requests.empty())return false;}
            lease->rows=source.requests(requests,n.at("columns"));if(!lease->rows)throw std::runtime_error("Request source returned a null cursor");
          }
        });
      } else if(kind=="input") {
        rows=std::make_shared<detail::FunctionRows>([once=n.at("one_row").get<bool>()](Json& row)mutable{if(!once)return false;once=false;row=Json::array();return true;});
      } else if(kind=="local_project") {
        rows=std::make_shared<detail::FunctionRows>([input=outputs.at(n.at("input").get<std::size_t>()),n,functions=functions_,runtime=runtime_](Json& row){
          Json source;if(!input->next(source))return false;row=Json::array();for(const auto& expr:n.at("expressions"))row.push_back(evaluate(expr,source,functions,runtime));return true;
        });
      } else {
        std::map<std::string,std::shared_ptr<SourceRows>> inputs;
        for(const auto& r:n.at("inputs"))inputs.emplace(r.at("name").get<std::string>(),outputs.at(r.at("node").get<std::size_t>()));
        rows=std::make_shared<detail::FunctionRows>([inputs=std::move(inputs),n,complete=completion_,lease=std::make_shared<detail::CursorLease>()](Json& row)mutable{
          if(!lease->rows)lease->rows=complete(n.at("sql").get<std::string>(),inputs);if(!lease->rows)throw std::runtime_error("Source returned a null cursor");return lease->rows->next(row);
        });
      }
      bool reentry=false;
      for(std::size_t next=id+1;next<nodes.size();++next) {
        if(kind=="duckdb"&&nodes[next].at("kind")=="duckdb")reentry=true;
        if(n.contains("source")&&nodes[next].contains("source")) {
          const auto& source=sources_.at(n.at("source").get<std::string>());
          const auto& other=sources_.at(nodes[next].at("source").get<std::string>());
          if(!source.concurrent&&(n.at("source")==nodes[next].at("source")||(!source.concurrency_key.empty()&&source.concurrency_key==other.concurrency_key)))reentry=true;
        }
      }
      if(reentry)rows=detail::replay_rows(std::move(rows));outputs.push_back(std::move(rows));
    }
    return outputs.at(result);
  }
};
} // namespace orchiddb

#endif
