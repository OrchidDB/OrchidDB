#include <erl_nif.h>
#include <dlfcn.h>
#include <stdint.h>
#include <string.h>
#include <stdlib.h>

static ErlNifMutex *loader_lock;
static void *cached_library;
static char *cached_path;
static int load(ErlNifEnv *env, void **priv, ERL_NIF_TERM info) {
    (void)env; (void)priv; (void)info;
    loader_lock = enif_mutex_create("orchiddb_loader");
    return loader_lock ? 0 : -1;
}
static ERL_NIF_TERM binary(ErlNifEnv *env, const char *text) {
    ERL_NIF_TERM term;
    size_t len = strlen(text);
    unsigned char *out = enif_make_new_binary(env, len, &term);
    if (len) memcpy(out, text, len);
    return term;
}
static ERL_NIF_TERM error(ErlNifEnv *env, const char *message) {
    return enif_make_tuple2(env, enif_make_atom(env, "error"), binary(env, message));
}
static ERL_NIF_TERM invoke_json(ErlNifEnv *env, int argc, const ERL_NIF_TERM argv[], const char *symbol) {
    ErlNifBinary path, request;
    if (argc != 2 || !enif_inspect_binary(env, argv[0], &path) || !enif_inspect_binary(env, argv[1], &request)) return enif_make_badarg(env);
    if (memchr(path.data, 0, path.size) || memchr(request.data, 0, request.size)) return error(env, "Embedded NUL is forbidden");
    char *p = enif_alloc(path.size + 1), *q = enif_alloc(request.size + 1);
    if (!p || !q) { if (p) enif_free(p); if (q) enif_free(q); return error(env, "Allocation failure"); }
    memcpy(p, path.data, path.size); p[path.size] = 0;
    memcpy(q, request.data, request.size); q[request.size] = 0;
    enif_mutex_lock(loader_lock);
    if (cached_path && strcmp(cached_path, p)) {
        enif_mutex_unlock(loader_lock); enif_free(p); enif_free(q);
        return error(env, "Only one compiler library may be loaded per VM");
    }
    void *library = cached_library ? cached_library : dlopen(p, RTLD_NOW | RTLD_LOCAL);
    if (!library) {
        ERL_NIF_TERM result = error(env, dlerror());
        enif_mutex_unlock(loader_lock); enif_free(p); enif_free(q); return result;
    }
    if (!cached_library) { cached_library = library; cached_path = p; }
    else enif_free(p);
    enif_mutex_unlock(loader_lock);
    uint32_t (*abi)(void) = dlsym(library, "orchiddb_abi_version");
    char *(*compile)(const char *) = dlsym(library, symbol);
    void (*release)(char *) = dlsym(library, "orchiddb_string_free");
    const char *(*revision)(void) = dlsym(library, "orchiddb_core_revision");
    if (!abi || !compile || !release || !revision || abi() != 2) {
        enif_free(q); return error(env, "Incompatible OrchidDB ABI");
    }
    char *response = compile(q);
    enif_free(q);
    if (!response) { return error(env, "Compiler returned null"); }
    ERL_NIF_TERM result = enif_make_tuple3(env, enif_make_atom(env, "ok"), binary(env, response), binary(env, revision()));
    release(response);
    /* The Rust compiler has a process-wide runtime. Keep dlopen references alive
       until VM exit; unloading code under its worker threads is unsafe. */
    return result;
}
static ERL_NIF_TERM compile_json(ErlNifEnv *env, int argc, const ERL_NIF_TERM argv[]) {
    return invoke_json(env, argc, argv, "orchiddb_execution_command");
}
static ERL_NIF_TERM statistics_json(ErlNifEnv *env, int argc, const ERL_NIF_TERM argv[]) {
    return invoke_json(env, argc, argv, "orchiddb_statistics_json");
}
static ERL_NIF_TERM remote_json(ErlNifEnv *env, int argc, const ERL_NIF_TERM argv[]) {
    return invoke_json(env, argc, argv, "orchiddb_remote_json");
}
static ErlNifFunc functions[] = {
    {"remote_json", 2, remote_json, ERL_NIF_DIRTY_JOB_IO_BOUND},
    {"compile_json", 2, compile_json, ERL_NIF_DIRTY_JOB_CPU_BOUND},
    {"statistics_json", 2, statistics_json, ERL_NIF_DIRTY_JOB_CPU_BOUND}
};
ERL_NIF_INIT(Elixir.OrchidDB.Native, functions, load, NULL, NULL, NULL)
