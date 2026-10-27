#include <orchiddb/orchiddb.hpp>
#include <cassert>
#include <iostream>
int main(){
  using namespace orchiddb;
  Json schema={{"functions",Json::array({{{"name","twice"},{"target","twice"},{"parameters",{"int64"}},{"returns","int64"},{"local",true}}})}};
  for(const auto& engine:{"none","duckdb"}) {
    SourceConnection graph(schema,engine,{},{{"twice",[](const std::vector<Json>& args){return Json(args.at(0).get<std::int64_t>()*2);}}},
      [](const std::string&,const std::map<std::string,std::shared_ptr<SourceRows>>&)->std::shared_ptr<SourceRows>{throw std::runtime_error("DuckDB initialized unexpectedly");});
    auto result=graph.query("RETURN twice(21) AS answer");Json row;
    assert(result->next(row));assert(row==Json::array({42}));assert(!result->next(row));result->close();
  }
  std::cout<<"Source-first local UDF passed without a DuckDB driver\n";
}
