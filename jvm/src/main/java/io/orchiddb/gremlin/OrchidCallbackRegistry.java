package io.orchiddb.gremlin;

import com.fasterxml.jackson.databind.ObjectMapper;
import java.io.*;
import java.net.*;
import java.nio.charset.StandardCharsets;
import java.util.*;
import java.util.concurrent.*;
import java.util.function.*;

/** Query-scoped handles for Java callbacks that cannot be serialized as source code.
 * The client invokes only the registered function. Graph access is borrowed from
 * the active relational JVM operator and its native transaction.
 */
public final class OrchidCallbackRegistry implements AutoCloseable {
    private static final ObjectMapper JSON=new ObjectMapper();
    private final ServerSocket server;
    private final String token=UUID.randomUUID().toString();
    private final Map<String,Object> callbacks=new ConcurrentHashMap<>();
    private final Set<Socket> active=ConcurrentHashMap.newKeySet();
    private final ExecutorService clients=Executors.newCachedThreadPool(r->{Thread t=new Thread(r,"orchiddb-java-callback");t.setDaemon(true);return t;});
    public OrchidCallbackRegistry() throws IOException {
        server=new ServerSocket(0,16,InetAddress.getByName("127.0.0.1"));
        Thread listener=new Thread(()->{
            while(!server.isClosed())try {Socket socket=server.accept();active.add(socket);clients.execute(()->serve(socket));}
            catch(IOException e){if(!server.isClosed())close();}
        },"orchiddb-java-callback-listener");listener.setDaemon(true);listener.start();
    }
    public String register(Object callback) {
        if(!(callback instanceof Supplier<?>||callback instanceof Function<?,?>||callback instanceof BiFunction<?,?,?>))
            throw new IllegalArgumentException("Unsupported Java callback: "+callback.getClass());
        String id=UUID.randomUUID().toString();callbacks.put(id,callback);
        return "{ Object... args -> io.orchiddb.gremlin.OrchidCallbackRegistry.invoke("+server.getLocalPort()+",'"+token+"','"+id+"',args,graph) }";
    }
    @SuppressWarnings("unchecked")
    private void serve(Socket socket) {
        OrchidGraph graph=null;
        try {
            socket.setSoTimeout(30000);
            var input=new BufferedReader(new InputStreamReader(socket.getInputStream(),StandardCharsets.UTF_8));
            var output=socket.getOutputStream();
            Map<String,Object> request=JSON.readValue(frame(input),Map.class);
            if(!token.equals(request.get("token")))throw new IllegalArgumentException("Invalid callback capability");
            Object callback=callbacks.get(request.get("id"));
            if(callback==null)throw new IllegalArgumentException("Callback handle expired");
            graph=OrchidGraph.forIr(input,output);
            List<?> args=(List<?>)request.get("args");
            Object result;
            if(callback instanceof Supplier<?> supplier){if(!args.isEmpty())throw new IllegalArgumentException("Supplier arity");result=supplier.get();}
            else if(callback instanceof BiFunction<?,?,?> function){if(args.size()!=2)throw new IllegalArgumentException("BiFunction arity");result=((BiFunction<Object,Object,Object>)function).apply(OrchidCodec.decode(args.get(0),graph),OrchidCodec.decode(args.get(1),graph));}
            else {if(args.size()!=1)throw new IllegalArgumentException("Function arity");result=((Function<Object,Object>)callback).apply(OrchidCodec.decode(args.get(0),graph));}
            write(output,OrchidCodec.fields("kind","result","ok",true,"value",OrchidCodec.encode(result,graph)));
        }catch(Throwable failure){
            // A disconnected or failed callback fails the calling operator; it is never retried.
            try{write(socket.getOutputStream(),Map.of("kind","result","ok",false,"error",failure.toString()));}catch(IOException ignored){}
        }finally{if(graph!=null)graph.abortFamily();active.remove(socket);try{socket.close();}catch(IOException ignored){}}
    }
    @SuppressWarnings("unchecked")
    public static Object invoke(int port,String token,String id,Object[] arguments,OrchidGraph graph) throws IOException {
        try(Socket socket=new Socket()) {
            socket.connect(new InetSocketAddress("127.0.0.1",port),5000);socket.setSoTimeout(30000);
            List<Object> args=new ArrayList<>();for(Object arg:arguments)args.add(OrchidCodec.encode(arg,graph));
            write(socket.getOutputStream(),Map.of("token",token,"id",id,"args",args));
            var input=new BufferedReader(new InputStreamReader(socket.getInputStream(),StandardCharsets.UTF_8));
            while(true) {
                Map<String,Object> response=JSON.readValue(frame(input),Map.class);
                if("native".equals(response.get("kind"))) {
                    if(((Number)response.get("graph")).longValue()!=0)throw new IOException("Callback cannot create an independent execution graph");
                    Map<String,Object> reply;
                    try {reply=OrchidCodec.fields("ok",true,"value",graph.callbackRequest((Map<String,Object>)response.get("request")));}
                    catch(RuntimeException error){reply=Map.of("ok",false,"error",error.toString());}
                    write(socket.getOutputStream(),reply);
                } else if("result".equals(response.get("kind"))) {
                    if(!Boolean.TRUE.equals(response.get("ok")))throw new IOException(String.valueOf(response.get("error")));
                    return OrchidCodec.decode(response.get("value"),graph);
                } else throw new IOException("Invalid callback frame");
            }
        }
    }
    private static String frame(Reader input)throws IOException {
        StringBuilder line=new StringBuilder();int c;
        while((c=input.read())!=-1){if(c=='\n')return line.toString();if(line.length()>=64*1024*1024)throw new IOException("Callback frame exceeds limit");line.append((char)c);}
        throw new EOFException("Java callback disconnected");
    }
    private static void write(OutputStream output,Object frame)throws IOException {output.write((JSON.writeValueAsString(frame)+"\n").getBytes(StandardCharsets.UTF_8));output.flush();}
    @Override public void close(){try{server.close();}catch(IOException ignored){}for(Socket socket:active)try{socket.close();}catch(IOException ignored){}callbacks.clear();clients.shutdownNow();}
}
