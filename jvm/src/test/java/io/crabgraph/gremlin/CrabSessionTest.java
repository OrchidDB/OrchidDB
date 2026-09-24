package io.crabgraph.gremlin;

import org.junit.Test;
import java.nio.file.*;
import java.util.concurrent.*;
import java.util.concurrent.atomic.*;
import static org.junit.Assert.*;

/** Transport tests use a delayed protocol peer to make cancellation timing deterministic. */
public class CrabSessionTest {
    @Test public void interruptedReplyIsDrainedBeforeNextRequest() throws Exception {
        Path directory=Files.createTempDirectory("crabgraph-protocol-");
        Path script=directory.resolve("peer.sh"),submitted=directory.resolve("submitted");
        Files.writeString(script,"#!/bin/sh\nwhile IFS= read -r line; do\ncase \"$line\" in *slow*) touch '"+submitted+"'; sleep 0.3;; esac\nprintf '%s\\n' '{\"ok\":true,\"value\":\"reply\"}'\ndone\n");
        assertTrue(script.toFile().setExecutable(true));
        try(CrabSession session=new CrabSession(script.toString(),null)) {
            AtomicBoolean cancelled=new AtomicBoolean(),interrupted=new AtomicBoolean();
            AtomicReference<Throwable> failure=new AtomicReference<>();
            Thread worker=new Thread(()->{
                try { session.call(CrabCodec.fields("op","slow")); }
                catch(CancellationException expected) { cancelled.set(true); interrupted.set(Thread.currentThread().isInterrupted()); }
                catch(Throwable unexpected) { failure.set(unexpected); }
            });
            worker.start();
            long deadline=System.nanoTime()+TimeUnit.SECONDS.toNanos(5);
            while(!Files.exists(submitted)&&System.nanoTime()<deadline) Thread.sleep(5);
            assertTrue("Protocol peer must have received request",Files.exists(submitted));
            worker.interrupt(); worker.join(3000);
            assertFalse(worker.isAlive()); assertNull(failure.get()); assertTrue(cancelled.get()); assertTrue(interrupted.get());
            assertEquals("reply",session.call(CrabCodec.fields("op","next")));
            Thread.currentThread().interrupt();
            try { session.call(CrabCodec.fields("op","never-sent")); fail(); }
            catch(CancellationException expected) { assertTrue(Thread.interrupted()); }
            finally { Thread.interrupted(); }
            assertEquals("reply",session.call(CrabCodec.fields("op","still-open")));
        } finally { Files.deleteIfExists(submitted); Files.deleteIfExists(script); Files.deleteIfExists(directory); }
    }
}
