package io.crabgraph.conformance;

import com.fasterxml.jackson.databind.*;
import com.fasterxml.jackson.databind.node.*;
import java.lang.reflect.*;
import java.nio.file.*;
import java.security.MessageDigest;
import java.util.*;
import org.apache.tinkerpop.gremlin.*;
import org.apache.tinkerpop.gremlin.process.traversal.TraversalEngine;
import org.junit.*;
import org.junit.runner.*;
import org.junit.runner.notification.*;

/** Runs unmodified pinned JUnit classes; assumptions/exclusions never become passes. */
public final class ProviderSuite {
    private static final ObjectMapper JSON = new ObjectMapper();
    private static final String REVISION = "fa698ba2aba8967dcd17eb61cb13648b934fab5b";
    private static String sha(byte[] bytes) throws Exception {
        return HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(bytes));
    }
    private static ArrayNode annotations(Description description) {
        ArrayNode result = JSON.createArrayNode();
        description.getAnnotations().forEach(a -> result.add(a.toString()));
        return result;
    }
    private static ArrayNode requirements(Description description) {
        ArrayNode result=JSON.createArrayNode();
        try {
            Method test=description.getTestClass().getMethod(description.getMethodName().split("\\[",2)[0]);
            Set<FeatureRequirement> required=new HashSet<>(Arrays.asList(test.getAnnotationsByType(FeatureRequirement.class)));
            for(LoadGraphWith fixture:test.getAnnotationsByType(LoadGraphWith.class))
                required.addAll(fixture.value().featuresRequired());
            for(FeatureRequirementSet bundle:test.getAnnotationsByType(FeatureRequirementSet.class))
                required.addAll(bundle.value().featuresRequired());
            required.stream().sorted(Comparator.comparing(r->r.featureClass().getName()+r.feature())).forEach(r->{
                ObjectNode row=result.addObject();
                row.put("feature_class",r.featureClass().getName());
                row.put("feature",r.feature()); row.put("supported",r.supported());
            });
        } catch (ReflectiveOperationException | NullPointerException ignored) {
            // Initialization failures have no corresponding original test method.
        }
        return result;
    }
    private static Class<?>[] suiteClasses(String name) throws Exception {
        String pkg = name.equals("StructureStandardSuite") ? "structure" : "process";
        Class<?> suite = Class.forName("org.apache.tinkerpop.gremlin." + pkg + "." + name);
        Field classes = suite.getDeclaredField("allTests");
        classes.setAccessible(true);
        return (Class<?>[]) classes.get(null);
    }
    private static String sourceHash(Class<?> klass, Path upstream) throws Exception {
        String name = klass.getName().split("\\$")[0];
        Path source = upstream.resolve("gremlin-test/src/main/java/" + name.replace('.', '/') + ".java");
        if (!Files.isRegularFile(source)) throw new IllegalArgumentException("Missing pinned source: " + source);
        return sha(Files.readAllBytes(source));
    }
    public static void main(String[] args) throws Exception {
        if (args.length < 3) throw new IllegalArgumentException("ProviderSuite <placeholders.json|SuiteName|class#method> <upstream-root> <output.json> [inventory]");
        String selection = args[0];
        Path upstream = Path.of(args[1]);
        boolean inventory = args.length > 3 && args[3].equals("inventory");
        boolean computer = selection.equals("ProcessComputerSuite") || Boolean.getBoolean("crabgraph.test.computer");
        GraphManager.setTraversalEngineType(computer ? TraversalEngine.Type.COMPUTER : TraversalEngine.Type.STANDARD);
        GraphManager.setGraphProvider(new NativeGraphProvider(computer));
        ObjectNode report = JSON.createObjectNode();
        report.put("upstream_revision", REVISION);
        report.put("profile", computer ? "jvm-graphcomputer" : "jvm-provider");
        report.put("selection", selection);
        report.put("inventory_only", inventory);
        report.put("run_complete", false);
        report.put("native_rust_traversal_evidence", false);
        report.put("upstream_assertions_modified", false);
        ObjectNode properties=report.putObject("effective_jvm_properties");
        for(String key:List.of("is.testing","assertNonDeterministic","crabgraph.test.computer","build.dir"))
            properties.put(key,System.getProperty(key,""));
        ArrayNode cases = report.putArray("cases");
        List<Request> requests = new ArrayList<>();
        Map<String,JsonNode> mappings = new HashMap<>();
        Map<String,String> hashes = new HashMap<>();
        if (selection.endsWith(".json")) {
            for (JsonNode row : JSON.readTree(Path.of(selection).toFile()).get("cases")) {
                Class<?> klass = Class.forName(row.get("java_class").asText());
                String method = row.get("java_method").asText();
                String hash = sourceHash(klass, upstream);
                if (!hash.equals(row.get("source_sha256").asText())) throw new IllegalStateException("Pinned test source hash mismatch: " + klass);
                mappings.put(klass.getName()+"#"+method, row);
                hashes.put(klass.getName(), hash);
                requests.add(Request.method(klass, method));
            }
        } else if (selection.contains("#")) {
            String[] parts = selection.split("#",2);
            Class<?> klass = Class.forName(parts[0]);
            hashes.put(klass.getName(), sourceHash(klass, upstream));
            requests.add(Request.method(klass,parts[1]));
        } else if (!selection.endsWith("Suite")) {
            Class<?> klass = Class.forName(selection);
            hashes.put(klass.getName(), sourceHash(klass, upstream));
            requests.add(Request.aClass(klass));
        } else {
            for (Class<?> klass : suiteClasses(selection)) {
                hashes.put(klass.getName(), sourceHash(klass, upstream));
                requests.add(Request.aClass(klass));
            }
        }
        RunListener listener = new RunListener() {
            final Map<Description,ObjectNode> active = new HashMap<>();
            private ObjectNode record(Description d) {
                return active.computeIfAbsent(d, key -> {
                    ObjectNode row = cases.addObject();
                    row.put("id",d.getClassName()+"#"+d.getMethodName());
                    row.put("source_sha256",hashes.get(d.getClassName()));
                    row.set("annotations", annotations(d));
                    row.set("feature_requirements",requirements(d));
                    row.put("status","running");
                    JsonNode mapping = mappings.get(d.getClassName()+"#"+d.getMethodName());
                    if (mapping != null) row.set("placeholder",mapping);
                    return row;
                });
            }
            private void checkpoint() {
                try { JSON.writerWithDefaultPrettyPrinter().writeValue(Path.of(args[2]).toFile(), report); }
                catch (Exception e) { throw new IllegalStateException(e); }
            }
            @Override public void testStarted(Description d) {
                // Parameterized runners can expose the same description for
                // distinct invocations. Preserve every execution separately.
                active.remove(d);
                record(d); checkpoint();
            }
            @Override public void testFailure(Failure f) { record(f.getDescription()).put("status","fail").put("error",f.getTrace()); }
            @Override public void testAssumptionFailure(Failure f) { record(f.getDescription()).put("status","skipped").put("reason",f.getMessage()); }
            @Override public void testIgnored(Description d) {
                active.remove(d);
                record(d).put("status","skipped").put("reason","JUnit @Ignore");
            }
            @Override public void testFinished(Description d) {
                ObjectNode row=record(d);
                if (row.path("status").asText().equals("running")) row.put("status","pass");
                System.err.println(row.get("status").asText()+" "+d);
                checkpoint();
            }
        };
        for (Request request : requests) {
            Runner runner = request.getRunner();
            if (inventory) describe(runner.getDescription(), cases, hashes);
            else {
                JUnitCore junit = new JUnitCore();
                junit.addListener(listener);
                junit.run(runner);
            }
        }
        report.put("run_complete", true);
        ObjectNode counts = report.putObject("counts");
        for (JsonNode row : cases) {
            String status = row.get("status").asText();
            counts.put(status,counts.path(status).asInt()+1);
        }
        JSON.writerWithDefaultPrettyPrinter().writeValue(Path.of(args[2]).toFile(),report);
        System.err.println(counts);
        if (counts.path("fail").asInt() > 0) System.exit(1);
    }
    private static void describe(Description d, ArrayNode cases, Map<String,String> hashes) {
        if (d.isTest()) {
            ObjectNode row = cases.addObject();
            row.put("id",d.getClassName()+"#"+d.getMethodName());
            row.put("source_sha256",hashes.get(d.getClassName()));
            row.put("status","not-run");
            row.set("annotations",annotations(d));
            row.set("feature_requirements",requirements(d));
        } else for (Description child : d.getChildren()) describe(child,cases,hashes);
    }
}
