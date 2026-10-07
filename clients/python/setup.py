from pathlib import Path
from setuptools import setup, Distribution
from wheel.bdist_wheel import bdist_wheel
class PlatformDistribution(Distribution):
    def has_ext_modules(self):
        return any((Path(__file__).parent / "src/orchiddb/native").glob("*"))
class PlatformWheel(bdist_wheel):
    def finalize_options(self):
        super().finalize_options()
        self.root_is_pure = not any((Path(__file__).parent / "src/orchiddb/native").glob("*"))
    def get_tag(self):
        python, abi, platform = super().get_tag()
        return ("py3", "none", platform) if not self.root_is_pure else (python, abi, platform)
setup(distclass=PlatformDistribution, cmdclass={"bdist_wheel": PlatformWheel})
