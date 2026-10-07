package io.orchiddb;

import static org.junit.jupiter.api.Assertions.*;

import io.orchiddb.internal.NativeLibrary;
import org.junit.jupiter.api.Test;

class NativeLibraryTest {
  @Test
  void normalizesJvmPlatformNames() {
    assertEquals("macos-aarch64", NativeLibrary.platform("Mac OS X", "aarch64"));
    assertEquals("macos-x86_64", NativeLibrary.platform("Darwin", "x86_64"));
    assertEquals("linux-x86_64", NativeLibrary.platform("Linux", "amd64"));
    assertEquals("windows-x86_64", NativeLibrary.platform("Windows 11", "amd64"));
    assertThrows(PlanningException.class, () -> NativeLibrary.platform("Linux", "riscv64"));
  }

  @Test
  void missingClassifierGivesActionableErrorWithoutNetwork() {
    var error = assertThrows(PlanningException.class, NativeLibrary::extract);
    assertTrue(error.getMessage().contains("classifier"));
  }
}
