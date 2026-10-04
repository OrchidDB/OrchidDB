import java.net.URL;
import java.net.URLClassLoader;
import java.nio.file.Path;
import java.util.ArrayList;
import org.apache.tinkerpop.gremlin.structure.Graph;

/** Isolate ArcadeDB's ANTLR 4.13 ABI from the upstream Gremlin parser's 4.9 ABI.
 * TinkerPop graph interfaces stay in the parent loader; assertions are unmodified.
 */
final class ArcadeProviderLoader extends URLClassLoader {
  ArcadeProviderLoader() throws Exception {
    super(urls(), ArcadeProviderLoader.class.getClassLoader());
  }
  private static URL[] urls() throws Exception {
    var entries=System.getProperty("java.class.path").split(java.io.File.pathSeparator);
    var urls=new ArrayList<URL>();
    for(var entry:entries) if(Path.of(entry).getFileName().toString().equals("antlr4-runtime-4.13.2.jar")) urls.add(Path.of(entry).toUri().toURL());
    if(urls.size()!=1) throw new IllegalStateException("Expected one ArcadeDB ANTLR 4.13.2 runtime");
    for(var entry:entries) if(Path.of(entry).getFileName().toString().startsWith("arcadedb-") && entry.endsWith(".jar")) urls.add(Path.of(entry).toUri().toURL());
    return urls.toArray(URL[]::new);
  }
  @Override protected Class<?> loadClass(String name, boolean resolve) throws ClassNotFoundException {
    synchronized(getClassLoadingLock(name)) {
      var type=findLoadedClass(name);
      if(type==null) {
        if(name.startsWith("com.arcadedb.") || name.startsWith("org.antlr.v4.")) type=findClass(name);
        else type=super.loadClass(name,false);
      }
      if(resolve) resolveClass(type);
      return type;
    }
  }
  Graph open(String directory) throws Exception {
    // Engine discovery uses ServiceLoader and the thread context loader.
    var thread=Thread.currentThread();var previous=thread.getContextClassLoader();
    try {
      thread.setContextClassLoader(this);
      return (Graph)loadClass("com.arcadedb.gremlin.ArcadeGraph").getMethod("open",String.class).invoke(null,directory);
    } finally {thread.setContextClassLoader(previous);}
  }
}
