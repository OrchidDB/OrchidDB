package io.crabgraph.gremlin;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.Path;
import java.time.Duration;
import java.util.*;
import java.util.concurrent.ExecutionException;
import org.apache.tinkerpop.gremlin.structure.io.graphson.GraphSONMapper;
import org.apache.tinkerpop.gremlin.structure.io.graphson.GraphSONVersion;

/** JSON-lines product entry point for trusted Gremlin-Groovy submissions on native graph state. */
public final class CrabJvm {
    private CrabJvm() { }
    public static void main(String[] args) throws Exception {
        String executable = System.getenv().getOrDefault("CRABGRAPH_JVM_STORE", "crabgraph-jvm-store");
        Path path = null;
        for (int i=0; i<args.length; i++) {
            if (args[i].equals("--store") && i+1<args.length) executable=args[++i];
            else if (args[i].equals("--path") && i+1<args.length) path=Path.of(args[++i]);
            else throw new IllegalArgumentException("Usage: CrabJvm [--store EXECUTABLE] [--path SNAPSHOT_FILE]");
        }
        ObjectMapper json = new ObjectMapper();
        var graphson = GraphSONMapper.build().version(GraphSONVersion.V3_0).create().createMapper();
        CrabGraph graph = path==null ? CrabGraph.open(executable) : CrabGraph.open(executable,path);
        try (CrabJvmExecutor executor = new CrabJvmExecutor(graph);
             BufferedReader input = new BufferedReader(new InputStreamReader(System.in,StandardCharsets.UTF_8))) {
            String line;
            while ((line=input.readLine())!=null) {
                Map<String,Object> response = new LinkedHashMap<>();
                response.put("protocol",1);
                response.put("execution_profile","crabgraph-jvm");
                try {
                    JsonNode request=json.readTree(line);
                    if (!request.path("script").isTextual()) throw new IllegalArgumentException("script must be a string");
                    Map<String,Object> bindings=new LinkedHashMap<>();
                    if (request.has("bindings")) {
                        if (!request.get("bindings").isObject()) throw new IllegalArgumentException("bindings must be an object of typed values");
                        request.get("bindings").fields().forEachRemaining(entry -> bindings.put(entry.getKey(),
                                CrabCodec.decode(json.convertValue(entry.getValue(),Object.class),graph)));
                    }
                    List<Object> values=executor.submit(request.get("script").asText(),bindings,
                            Duration.ofMillis(request.path("timeout_ms").asLong(30_000))).get();
                    response.put("ok",true);
                    // GraphSON preserves arbitrary map keys, graph identity, sets, paths and numeric widths.
                    response.put("result",json.readTree(graphson.writeValueAsString(values)));
                } catch (Exception failure) {
                    Throwable cause=failure instanceof ExecutionException && failure.getCause()!=null ? failure.getCause() : failure;
                    response.put("ok",false);
                    response.put("error",cause.getClass().getName()+": "+cause.getMessage());
                }
                System.out.println(json.writeValueAsString(response));
                System.out.flush();
            }
        }
    }
}
