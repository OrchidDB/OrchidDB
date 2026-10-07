"""Local regression: CRLF checkouts must create the same classifier JAR."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

spec = importlib.util.spec_from_file_location('package_native', Path(__file__).with_name('package-native.py'))
package_native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package_native)


class CanonicalLicenseTest(unittest.TestCase):
    def test_crlf_checkout_produces_identical_jar(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'native').mkdir()
            (root / 'native/CORE_REVISION').write_text('a' * 40 + '\n')
            library = root / 'liborchiddb_java.so'
            library.write_bytes(b'fixture compiler')
            outputs = []
            with patch.object(package_native, 'ROOT', root):
                for index, license in enumerate([b'License\nTerms\n', b'License\r\nTerms\r\n']):
                    (root / 'LICENSE.md').write_bytes(license)
                    output = root / f'{index}.jar'
                    package_native.package('linux-x86_64', library, '1.2.3', output)
                    outputs.append(output.read_bytes())
            self.assertEqual(*outputs)
            with zipfile.ZipFile(root / '1.jar') as archive:
                self.assertEqual(archive.read('META-INF/LICENSE.md'), b'License\nTerms\n')


if __name__ == '__main__':
    unittest.main()
