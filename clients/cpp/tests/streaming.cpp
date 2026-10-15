#include <orchiddb/orchiddb.hpp>
#include <curl/curl.h>
#include <cassert>
#include <fstream>
#include <iostream>
using orchiddb::Json;
static size_t receive(char* p,size_t a,size_t b,void* out){static_cast<std::string*>(out)->append(p,a*b);return a*b;}
int main(){
 const char* path=std::getenv("ORCHIDDB_STREAM_FIXTURE");if(!path){std::cout<<"SKIP: live Kafka/SpiceDB fixture required\n";return 0;}
 Json f;std::ifstream(path)>>f;curl_global_init(CURL_GLOBAL_DEFAULT);
 auto auth=[&](const Json& subject,const Json& checks){
  CURL* c=curl_easy_init();assert(c);auto body=Json({{"subject",subject},{"checks",checks}}).dump();std::string output;
  auto url=f.at("auth_url").get<std::string>();curl_easy_setopt(c,CURLOPT_URL,url.c_str());curl_easy_setopt(c,CURLOPT_POSTFIELDS,body.c_str());
  curl_easy_setopt(c,CURLOPT_WRITEFUNCTION,receive);curl_easy_setopt(c,CURLOPT_WRITEDATA,&output);curl_easy_setopt(c,CURLOPT_TIMEOUT,10L);
  auto headers=curl_slist_append(nullptr,"Content-Type: application/json");curl_easy_setopt(c,CURLOPT_HTTPHEADER,headers);
  auto status=curl_easy_perform(c);long code=0;curl_easy_getinfo(c,CURLINFO_RESPONSE_CODE,&code);curl_slist_free_all(headers);curl_easy_cleanup(c);
  if(status!=CURLE_OK||code!=200)throw std::runtime_error("authorization route failed");return Json::parse(output).get<std::vector<bool>>();
 };
 auto change=[](std::string key,int version){return Json{{"table","docs"},{"key",key},{"version",version},{"data",{{"name",key+"-"+std::to_string(version)}}}};};
 Json options={{"kafka",f.at("kafka")},{"topic",f.at("topic_prefix").get<std::string>()+"-cpp"},{"source",f.at("source")}};
 auto subject=f.at("subject");
 orchiddb::StreamConnection c(f.at("schema"),options,auth);
 bool denied=false;try{c.insert(change("blocked",1),subject);}catch(const std::exception& e){denied=std::string(e.what()).find("denied")!=std::string::npos;}assert(denied);
 c.insert(change("a",1),subject);c.insert_batch(Json::array({change("b",1),change("hidden",1),change("a",2)}),subject);
 auto query=c.query(f.at("query"),subject);
 auto hybrid=c.query_hybrid(f.at("query"),subject,{{"source",f.at("source")},{"offsets",f.at("offsets")},{"changes",Json::array({change("a",1),change("b",1)})}});
 c.subscribe(f.at("stream_query"),subject,f.at("offsets"));Json frames=Json::array(),offsets;
 for(int i=0;i<20;++i){auto b=c.next_batch(16,100);for(auto frame:b.at("frames"))frames.push_back(frame);offsets=b.at("offsets");if(offsets.at("0")==4)break;}
 assert(frames.size()==3&&offsets.at("0")==4);c.unsubscribe();c.close();
 std::ofstream("/tmp/orchid-stream-cpp-result.json")<<Json({{"query",query},{"hybrid",hybrid},{"frames",frames},{"offsets",offsets}}).dump();
 orchiddb::StreamConnection bad(f.at("schema"),options,[](const Json&,const Json&){return std::vector<bool>{};});
 bool failed=false;try{bad.insert(change("a",3),subject);}catch(const std::exception&){failed=true;}assert(failed);
 curl_global_cleanup();std::cout<<"Kafka/SpiceDB streaming passed\n";
}
