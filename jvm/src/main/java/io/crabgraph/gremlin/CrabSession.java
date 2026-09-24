package io.crabgraph.gremlin;

import com.fasterxml.jackson.core.type.TypeReference;
import com.fasterxml.jackson.databind.ObjectMapper;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.util.Map;
import java.util.concurrent.*;

/** One private native process; interrupted requests are drained before another request is accepted. */
final class CrabSession implements AutoCloseable {
    private final Process process;
    private final BufferedWriter input;
    private final BlockingQueue<Object> replies = new LinkedBlockingQueue<>();
    private final ObjectMapper mapper = new ObjectMapper();
    private volatile boolean closed;
    private final boolean borrowed;
    private final CrabSession owner;
    private final long graphId;
    CrabSession(String executable, java.nio.file.Path path) {
        borrowed=false; owner=null; graphId=0;
        try {
            ProcessBuilder builder=path==null?new ProcessBuilder(executable):new ProcessBuilder(executable,"--path",path.toString());
            process=builder.redirectError(ProcessBuilder.Redirect.INHERIT).start();
            input=new BufferedWriter(new OutputStreamWriter(process.getOutputStream(),StandardCharsets.UTF_8));
            Thread reader=new Thread(()->{
                try (BufferedReader stream=new BufferedReader(new InputStreamReader(process.getInputStream(),StandardCharsets.UTF_8))) {
                    String line; while((line=stream.readLine())!=null) replies.put(line);
                    replies.offer(new EOFException("Native store exited"));
                } catch(Exception e) { replies.offer(e); }
            },"crabgraph-native-replies");
            reader.setDaemon(true); reader.start();
        } catch(IOException e) { throw new IllegalStateException("Cannot start Crabgraph native store: "+executable,e); }
    }
    /** Borrow a native graph owned by the relational executor over a callback channel. */
    CrabSession(BufferedReader source, OutputStream sink) {
        borrowed=true; owner=null; graphId=0;
        process=null;
        input=new BufferedWriter(new OutputStreamWriter(sink,StandardCharsets.UTF_8));
        Thread reader=new Thread(()->{
            try {
                String line;
                while((line=source.readLine())!=null) replies.put(line);
                replies.offer(new EOFException("Relational executor disconnected"));
            } catch(Exception failure) { replies.offer(failure); }
        },"crabgraph-ir-native-replies");
        reader.setDaemon(true); reader.start();
    }
    private CrabSession(CrabSession owner,long graphId) {
        this.owner=owner; this.graphId=graphId; borrowed=true; process=null; input=owner.input;
    }
    boolean isBorrowed() { return borrowed; }
    CrabSession fork() {
        CrabSession root=owner==null?this:owner;
        Number id=(Number)root.call(Map.of("op","createExecutionGraph"));
        return new CrabSession(root,id.longValue());
    }
    Object call(Map<String,Object> request) {
        if(closed) throw new IllegalStateException("Native graph is closed");
        return owner==null?callForGraph(graphId,request):owner.callForGraph(graphId,request);
    }
    private synchronized Object callForGraph(long graphId,Map<String,Object> request) {
        if(closed) throw new IllegalStateException("Native graph is closed");
        if(Thread.currentThread().isInterrupted()) throw new CancellationException("Native request cancelled before submission");
        try {
            input.write(mapper.writeValueAsString(borrowed?Map.of("kind","native","graph",graphId,"request",request):request)); input.newLine(); input.flush();
            Object reply=replies.poll(Long.getLong("crabgraph.native.timeoutSeconds",120),TimeUnit.SECONDS);
            if(reply==null) throw new IOException("Native store response timed out");
            if(reply instanceof Exception) throw new IOException("Native store disconnected",(Exception)reply);
            Map<String,Object> response=mapper.readValue((String)reply,new TypeReference<Map<String,Object>>(){});
            if(!Boolean.TRUE.equals(response.get("ok"))) throw new NativeOperationException(String.valueOf(response.get("error")));
            return response.get("value");
        } catch(NativeOperationException e) { throw e; }
        catch(InterruptedException e) {
            CancellationException cancellation=new CancellationException("Native operation cancelled");
            try { drainInterruptedReply(); }
            catch(IOException failedRecovery) { close(); cancellation.addSuppressed(failedRecovery); }
            finally { Thread.currentThread().interrupt(); }
            throw cancellation;
        }
        catch(IOException e) { close(); throw new IllegalStateException("Native protocol failure; session terminated",e); }
    }
    private void drainInterruptedReply() throws IOException {
        long timeout=Long.getLong("crabgraph.native.cancelDrainMillis",5000);
        long deadline=System.nanoTime()+TimeUnit.MILLISECONDS.toNanos(timeout);
        while(true) {
            long remaining=deadline-System.nanoTime();
            if(remaining<=0) throw new IOException("Timed out resynchronizing cancelled native request");
            try {
                Object reply=replies.poll(remaining,TimeUnit.NANOSECONDS);
                if(!(reply instanceof String)) throw new IOException("Native session disconnected while cancelling",reply instanceof Exception?(Exception)reply:null);
                Map<String,Object> response=mapper.readValue((String)reply,new TypeReference<Map<String,Object>>(){});
                if(!(response.get("ok") instanceof Boolean)) throw new IOException("Invalid native response while cancelling");
                return;
            } catch(InterruptedException repeated) { /* Keep draining the already-submitted operation. */ }
        }
    }
    boolean isAlive() { return !closed&&(process==null||process.isAlive()); }
    void abort() {
        closed=true; if(process!=null) process.destroyForcibly();
        replies.offer(new EOFException("Native graph aborted"));
    }
    public void close() {
        closed=true;
        if(process==null) { replies.offer(new EOFException("IR graph closed")); return; }
        process.destroy();
        try { if(!process.waitFor(2,TimeUnit.SECONDS)) process.destroyForcibly(); }
        catch(InterruptedException e) { process.destroyForcibly(); Thread.currentThread().interrupt(); }
        replies.offer(new EOFException("Native graph closed"));
    }
    static final class NativeOperationException extends IllegalArgumentException {
        NativeOperationException(String message) { super(message); }
    }
}
