import java.net.*;
import java.nio.file.*;
import java.util.*;

/** Load the original 3.7.4 assertions with their original datetime helper ABI.
 * ArcadeDB and traversal execution still use the provider's TinkerPop 3.8.1.
 * No assertion class or expected value is rewritten.
 */
public final class ArcadeGremlinBootstrap {
  public static void main(String[] args) throws Exception {
    var entries=System.getProperty("java.class.path").split(java.io.File.pathSeparator);
    var urls=new ArrayList<URL>();URL legacyCore=null;
    for(var entry:entries) {
      var path=Path.of(entry);
      if(Files.isDirectory(path) || path.getFileName().toString().equals("gremlin-test-3.7.4.jar")) urls.add(path.toUri().toURL());
      if(path.getFileName().toString().equals("gremlin-core-3.7.4.jar")) legacyCore=path.toUri().toURL();
    }
    if(legacyCore==null) throw new IllegalStateException("Missing original TinkerPop 3.7.4 datetime helper");
    final URL core=legacyCore;
    try(var helperLoader=new URLClassLoader(new URL[]{core},null);
        var loader=new URLClassLoader(urls.toArray(URL[]::new),ArcadeGremlinBootstrap.class.getClassLoader()) {
          @Override protected Class<?> loadClass(String name,boolean resolve) throws ClassNotFoundException {
            synchronized(getClassLoadingLock(name)) {
              if(name.equals("org.apache.tinkerpop.gremlin.util.DatetimeHelper")) return helperLoader.loadClass(name);
              var type=findLoadedClass(name);
              if(type==null) {
                if(name.startsWith("UpstreamGremlin") || name.startsWith("NativeJUnitAssertions") || name.equals("ArcadeProviderLoader") || name.startsWith("org.apache.tinkerpop.gremlin.features.")) type=findClass(name);
                else type=super.loadClass(name,false);
              }
              if(resolve) resolveClass(type);return type;
            }
          }
        }) {
      Thread.currentThread().setContextClassLoader(loader);
      try {loader.loadClass("UpstreamGremlin").getMethod("main",String[].class).invoke(null,(Object)args);}
      catch(java.lang.reflect.InvocationTargetException error) {throw new RuntimeException(error.getCause());}
    }
  }
}
