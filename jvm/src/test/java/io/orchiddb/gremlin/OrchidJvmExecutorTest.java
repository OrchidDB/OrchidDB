package io.orchiddb.gremlin;

import static org.junit.Assert.*;
import java.math.BigInteger;
import java.nio.file.Files;
import java.time.Duration;
import java.util.*;
import java.util.concurrent.*;
import org.junit.Test;
import org.junit.Assume;
import org.apache.tinkerpop.gremlin.process.traversal.AnonymousTraversalSource;
import org.apache.tinkerpop.gremlin.util.function.Lambda;

public class OrchidJvmExecutorTest {
    private static final Duration TIMEOUT=Duration.ofSeconds(20);
    private String binary() {
        String path=System.getenv("ORCHIDDB_JVM_STORE");
        Assume.assumeTrue("Native store binary required",path!=null);
        return path;
    }
    @Test public void callbacksReadMutateAndCaptureValues() throws Exception {
        try (var session=new OrchidJvmExecutor(OrchidGraph.open(binary()))) {
            assertEquals(List.of("renamed"), session.submit(
                    "g.addV('person').property('name',name).map { t -> t.get().property('name','renamed'); t.get().value('name') }",
                    Map.of("name","original"),TIMEOUT).get());
            assertEquals(List.of("renamed"),session.submit("g.V().values('name')",Map.of(),TIMEOUT).get());
        }
    }
    @Test public void failedSubmissionRollsBackAndSessionRemainsUsable() throws Exception {
        try (var session=new OrchidJvmExecutor(OrchidGraph.open(binary()))) {
            session.submit("g.addV('retained')",Map.of(),TIMEOUT).get();
            try {
                session.submit("g.addV('discarded').map { throw new IllegalStateException('callback-failed') }",Map.of(),TIMEOUT).get();
                fail("Expected callback exception");
            } catch (ExecutionException failure) { assertTrue(failure.toString().contains("callback-failed")); }
            assertEquals(List.of("retained"),session.submit("g.V().label()",Map.of(),TIMEOUT).get());
        }
    }
    @Test public void remoteBytecodeExecutesUpstreamLambdaOnNativeElements() throws Exception {
        try (var session=new OrchidJvmExecutor(OrchidGraph.open(binary()))) {
            session.submit("g.addV('person').property('name','four')",Map.of(),TIMEOUT).get();
            try (var g=AnonymousTraversalSource.traversal().withRemote(session.remoteConnection())) {
                assertEquals(List.of(4),g.V().map(Lambda.function("it.get().value('name').length()")).toList());
            }
        }
    }
    @Test public void typedBindingsRetainPrecisionMapKeysAndSetIdentity() throws Exception {
        try (var session=new OrchidJvmExecutor(OrchidGraph.open(binary()))) {
            BigInteger large=new BigInteger("987654321012345678909876543210123456789");
            Map<Object,Object> map=new LinkedHashMap<>(); map.put(1,"int"); map.put(1L,"long"); map.put(large,Set.of("x"));
            assertEquals(List.of(map),session.submit("g.inject(seed)",Map.of("seed",map),TIMEOUT).get());
            assertEquals(List.of(large),session.submit("g.withSack(seed).inject(1).sack()",Map.of("seed",large),TIMEOUT).get());
        }
    }
    @Test public void sequentialSubmissionsObservePriorCommittedWrites() throws Exception {
        try (var session=new OrchidJvmExecutor(OrchidGraph.open(binary()))) {
            var first=session.submit("g.addV('one')",Map.of(),TIMEOUT);
            var second=session.submit("g.V().count()",Map.of(),TIMEOUT);
            first.get(); assertEquals(List.of(1L),second.get());
        }
    }
    @Test public void cancelledPersistentSubmissionCannotCommitPartialWrites() throws Exception {
        String store=binary(); var directory=Files.createTempDirectory("orchiddb-jvm-cancel");
        var snapshot=directory.resolve("graph.snapshot");
        try {
            try (var session=new OrchidJvmExecutor(OrchidGraph.open(store,snapshot))) {
                session.submit("g.addV('committed')",Map.of(),TIMEOUT).get();
                try {
                    session.submit("g.addV('uncommitted').iterate(); while (true) { }",Map.of(),Duration.ofMillis(500)).get();
                    fail("Expected deadline");
                } catch (ExecutionException failure) { assertTrue(failure.getCause() instanceof TimeoutException); }
                assertTrue(session.submit("g.V()",Map.of(),TIMEOUT).isCompletedExceptionally());
            }
            try (var graph=OrchidGraph.open(store,snapshot)) {
                assertEquals(List.of("committed"),graph.traversal().V().label().toList());
            }
        } finally {
            try(var paths=Files.walk(directory)) { paths.sorted(Comparator.reverseOrder()).forEach(p->{try { Files.delete(p); } catch(Exception ignored) {}}); }
        }
    }
}
