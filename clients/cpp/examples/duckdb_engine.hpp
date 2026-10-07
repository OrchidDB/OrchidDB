#pragma once
#include <orchiddb/orchiddb.hpp>
#include <duckdb.h>
#include <cerrno>
#include <condition_variable>
#include <mutex>
#include <thread>
#include <chrono>

/** Example borrowed-connection adapter. DuckDB dependency belongs to application. */
class DuckDBEngine final : public orchiddb::ExecutionEngine {
  duckdb_connection connection_;
  struct State {
    duckdb_arrow result{};
    ~State() { if(result) duckdb_destroy_arrow(&result); }
  };
public:
  explicit DuckDBEngine(duckdb_connection connection) : connection_(connection) {}
  std::string id() const override { return "caller-duckdb"; }
  std::string dialect() const override { return "duckdb"; }
  orchiddb::Json collect_statistics(const orchiddb::Json& work) override {
    if (work.at("dialect") != dialect()) throw std::invalid_argument("Wrong SQL dialect");
    std::mutex mutex;
    std::condition_variable wake;
    bool done = false;
    std::thread deadline([&] {
      std::unique_lock<std::mutex> lock(mutex);
      if (!wake.wait_for(lock, std::chrono::milliseconds(work.at("timeout_ms").get<uint64_t>()), [&] { return done; })) {
        do {
          duckdb_interrupt(connection_);
        } while (!wake.wait_for(lock, std::chrono::milliseconds(10), [&] { return done; }));
      }
    });
    struct Cleanup {
      std::mutex& mutex; std::condition_variable& wake; bool& done; std::thread& thread;
      ~Cleanup() { { std::lock_guard<std::mutex> lock(mutex); done = true; } wake.notify_one(); thread.join(); }
    } cleanup{mutex, wake, done, deadline};
    auto sql = "SELECT to_json(s) FROM (" + work.at("sql").get<std::string>() + ") s LIMIT " + std::to_string(work.at("max_rows").get<uint64_t>());
    duckdb_result result{};
    auto status = duckdb_query(connection_, sql.c_str(), &result);
    struct ResultCleanup { duckdb_result& result; ~ResultCleanup() { duckdb_destroy_result(&result); } } result_cleanup{result};
    if (status != DuckDBSuccess) throw std::runtime_error(duckdb_result_error(&result));
    orchiddb::Json rows = orchiddb::Json::array();
    size_t bytes = 0;
    bool truncated = false;
    const auto max_rows = work.at("max_rows").get<uint64_t>();
    const auto max_bytes = work.at("max_bytes").get<uint64_t>();
    for (idx_t row = 0; row < duckdb_row_count(&result) && row < max_rows; ++row) {
      std::unique_ptr<char, decltype(&duckdb_free)> text(duckdb_value_varchar(&result, 0, row), duckdb_free);
      if (!text) throw std::runtime_error("Statistics row is null");
      const auto length = std::char_traits<char>::length(text.get());
      if (bytes + length > max_bytes) { truncated = true; break; }
      bytes += length;
      rows.push_back(orchiddb::Json::parse(text.get()));
    }
    return {{"rows", rows}, {"truncated", truncated}};
  }
  orchiddb::ArrowResult execute(const orchiddb::CompiledQuery& plan) override {
    if(plan.dialect != dialect()) throw std::invalid_argument("Wrong SQL dialect");
    auto state=std::make_unique<State>();
    if(duckdb_query_arrow(connection_,plan.sql.c_str(),&state->result)!=DuckDBSuccess)
      throw std::runtime_error(duckdb_query_arrow_error(state->result));
    ArrowArrayStream stream{};
    stream.private_data=state.release();
    stream.get_schema=[](ArrowArrayStream* s,ArrowSchema* out) {
      auto ptr=reinterpret_cast<duckdb_arrow_schema>(out);
      return duckdb_query_arrow_schema(static_cast<State*>(s->private_data)->result,&ptr)==DuckDBSuccess?0:EIO;
    };
    stream.get_next=[](ArrowArrayStream* s,ArrowArray* out) {
      auto ptr=reinterpret_cast<duckdb_arrow_array>(out);
      return duckdb_query_arrow_array(static_cast<State*>(s->private_data)->result,&ptr)==DuckDBSuccess?0:EIO;
    };
    stream.get_last_error=[](ArrowArrayStream* s) {return duckdb_query_arrow_error(static_cast<State*>(s->private_data)->result);};
    stream.release=[](ArrowArrayStream* s) {delete static_cast<State*>(s->private_data);s->release=nullptr;s->private_data=nullptr;};
    return orchiddb::ArrowResult(stream);
  }
};
