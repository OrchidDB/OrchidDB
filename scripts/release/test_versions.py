import json
import unittest
from versions import update_manifest


class VersionsTest(unittest.TestCase):
    def test_cargo_preserves_dependency_prefix_collision(self):
        original = '[package]\nversion = "0.4.4"\n[dependencies]\nchrono = "0.4.44"\nother = "0.4.4"\norchiddb = { version = "=0.4.4", path = "../.." }\n'
        result = update_manifest('Cargo.toml', original, '0.4.4', '0.5.0')
        self.assertIn('version = "0.5.0"', result)
        self.assertIn('chrono = "0.4.44"', result)
        self.assertIn('other = "0.4.4"', result)
        self.assertIn('version = "=0.5.0"', result)

    def test_lock_preserves_unrelated_matching_version_and_checksum(self):
        third_party = {'version': '0.4.4', 'resolved': 'https://registry.npmjs.org/chalk-template/-/chalk-template-0.4.4.tgz', 'integrity': 'keep'}
        original = {'name': '@orchiddb/client', 'version': '0.4.4', 'packages': {'': {'version': '0.4.4'}, 'test': {'name': '@orchiddb/client-tests', 'version': '0.4.4'}, 'node_modules/chalk-template': third_party}}
        result = json.loads(update_manifest('package-lock.json', json.dumps(original), '0.4.4', '0.5.0'))
        self.assertEqual(result['packages']['node_modules/chalk-template'], third_party)
        self.assertEqual(result['packages']['test']['version'], '0.5.0')
        self.assertEqual(result['packages']['']['version'], '0.5.0')

    def test_maven_preserves_other_dependency(self):
        original = '<artifactId>orchiddb-parent</artifactId><version>0.4.4</version><artifactId>other</artifactId><version>0.4.4</version>'
        self.assertEqual(update_manifest('pom.xml', original, '0.4.4', '0.5.0'), original.replace('parent</artifactId><version>0.4.4', 'parent</artifactId><version>0.5.0'))
