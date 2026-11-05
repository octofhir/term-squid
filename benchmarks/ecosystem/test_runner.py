"""Guard the setup adapter: official cases and expectations cannot be silently weakened."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('ecosystem', ROOT / 'benchmarks/ecosystem.py')
RUNNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUNNER)


class ProfileTests(unittest.TestCase):
    def test_expected_inventory_uses_only_upstream_mode_and_version_not_passes(self):
        source = {'suites': [{'name': 'general', 'mode': 'general', 'tests': [
            {'name': 'ordinary'}, {'name': 'r4', 'version': '4.0'},
            {'name': 'r5', 'version': '!4.0'}, {'name': 'external', 'mode': 'tx.fhir.org'},
        ]}, {'name': 'clinical', 'mode': 'snomed', 'tests': [{'name': 'clinical'}]}]}
        self.assertEqual(RUNNER.expected_cases(source, ['general', 'clinical'], ['general'], '4.0.1'),
                         ['general/ordinary', 'general/r4'])
        self.assertEqual(RUNNER.expected_cases(source, ['general'], ['general'], '5.0.0'),
                         ['general/ordinary', 'general/r5'])
        source['suites'][0]['tests'][0]['version'] = 'unknown'
        with self.assertRaises(ValueError):
            RUNNER.expected_cases(source, ['general'], ['general'], '4.0.1')

    def test_preloading_changes_only_selected_suite_setup(self):
        source = {'suites': [{'name': 'case', 'setup': ['cs.json', 'vs.json'], 'tests': [
            {'name': 'valid', 'operation': 'validate-code', 'request': 'valid.json', 'response': 'expected.json'},
            {'name': 'negative', 'http-code': '4xx', 'response': 'negative.json', 'feature': 'test-feature'},
        ]}, {'name': 'other', 'setup': [], 'tests': []}], 'introduction': 'upstream'}
        before = json.dumps(source)
        profile, setup = RUNNER.preloaded_manifest(source, ['case'])
        self.assertEqual(json.dumps(source), before)
        self.assertEqual(profile['suites'][0]['tests'], source['suites'][0]['tests'])
        self.assertEqual(profile['suites'][0]['setup'], [])
        self.assertEqual(setup, ['cs.json', 'vs.json'])
        self.assertEqual(profile['introduction'], source['introduction'])
        with self.assertRaises(ValueError):
            RUNNER.preloaded_manifest(source, ['unknown'])

    def test_only_loopback_targets_are_accepted(self):
        for target in ['http://127.0.0.1:123/r4', 'http://localhost/r4', 'http://[::1]/r4']:
            self.assertEqual(RUNNER.local_url(target), target)
        for target in ['https://tx.fhir.org/r4', 'http://127.0.0.1.example.org/r4',
                       'http://user:password@localhost/r4', 'file:///tmp/server', 'http://localhost/r4?test=1']:
            with self.assertRaises(ValueError):
                RUNNER.local_url(target)

    def test_result_inventory_retains_failed_ignored_and_duplicate_case_names(self):
        folder = ROOT / '.benchmarks/ecosystem-unit'
        folder.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=folder) as temporary:
            path = Path(temporary) / 'result.json'
            path.write_text(json.dumps({'suites': [{'name': 'case', 'tests': [
                {'name': 'one', 'status': 'pass'}, {'name': 'one', 'status': 'pass'},
                {'name': 'two', 'status': 'fail'}, {'name': 'three', 'status': 'ignored'},
            ]}]}))
            result = RUNNER.results_summary(path)
            self.assertEqual(result['pass'], 2)
            self.assertEqual(result['fail'], 1)
            self.assertEqual(result['ignored'], 1)
            self.assertEqual(result['cases'], ['case/one', 'case/one', 'case/two', 'case/three'])


if __name__ == '__main__':
    unittest.main()
