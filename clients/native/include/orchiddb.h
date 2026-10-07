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
/* ABI 2. Calls may be concurrent. Compilation runs on a bounded native worker pool. */
ORCHIDDB_API uint32_t orchiddb_abi_version(void);
/* Borrowed static UTF-8 NUL-terminated strings. Do not free. */
ORCHIDDB_API const char *orchiddb_version(void);
ORCHIDDB_API const char *orchiddb_core_revision(void);
/* SDK implementation transport, not a customer query API. Clients register
 * schema on their connection and submit query text separately.
 * Input: UTF-8 NUL-terminated internal command JSON.
 * Output: owned UTF-8 JSON envelope; free once with orchiddb_string_free.
 * Invalid input returns an error envelope; invalid pointers are a caller error. */
ORCHIDDB_API char *orchiddb_execution_command(const char *input);
struct ArrowArrayStream;
/* Bind an Arrow C stream using {"plan": compiled_plan, "relation": name}.
 * If both arguments are non-null, ownership of the stream moves into this call,
 * including on errors: its release callback is cleared and imported resources
 * are released here. Null arguments leave stream ownership with the caller.
 * Returns the same owned JSON envelope as orchiddb_execution_command. No SQL is executed. */
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
