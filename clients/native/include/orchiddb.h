#ifndef ORCHIDDB_COMPILER_H
#define ORCHIDDB_COMPILER_H
#include <stdint.h>
#ifdef _WIN32
# define ORCHIDDB_API __declspec(dllimport)
#else
# define ORCHIDDB_API
#endif
#ifdef __cplusplus
extern "C" {
#endif
/* ABI 1. Calls may be concurrent. Compilation runs on a bounded native worker pool. */
ORCHIDDB_API uint32_t orchiddb_abi_version(void);
/* Borrowed static UTF-8 NUL-terminated strings. Do not free. */
ORCHIDDB_API const char *orchiddb_version(void);
ORCHIDDB_API const char *orchiddb_core_revision(void);
/* Input: UTF-8 NUL-terminated compiler protocol v1 JSON. The "bind" operation
 * accepts island results as typed JSON rows or base64 Arrow IPC.
 * Output: owned UTF-8 JSON {"ok":true,"result":{version,dialect,sql,fields}}
 *      or {"ok":false,"error":"..."}. Free once using orchiddb_string_free.
 * Null/invalid UTF-8/invalid requests return an error envelope. Invalid pointers
 * are a caller error. No connection setup, execution, or implicit downloads. */
ORCHIDDB_API char *orchiddb_compile_json(const char *input);
struct ArrowArrayStream;
/* Bind an Arrow C stream using {"plan": compiled_plan, "relation": name}.
 * If both arguments are non-null, ownership of the stream moves into this call,
 * including on errors: its release callback is cleared and imported resources
 * are released here. Null arguments leave stream ownership with the caller.
 * Returns the same owned JSON envelope as compile_json. No SQL is executed. */
ORCHIDDB_API char *orchiddb_bind_arrow_json(const char *input, struct ArrowArrayStream *stream);
/* Statistics coordinator: begin/next/submit/finish/cancel/install/release/compile.
 * Accepts bounded Arrow IPC or row samples; same response envelope and ownership. */
ORCHIDDB_API char *orchiddb_statistics_json(const char *input);
/* Explicit HTTP session lifecycle and typed prepared requests. Same envelope
 * and ownership. Commands: open/execute/clear_metadata_cache/close.
 * Only execute performs HTTP I/O; compilation never connects to a service. */
ORCHIDDB_API char *orchiddb_remote_json(const char *input);
ORCHIDDB_API void orchiddb_string_free(char *response);
#ifdef __cplusplus
}
#endif
#endif
