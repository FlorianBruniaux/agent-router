"""Offline checks for the U02 evidence projection and fail-closed verdict."""
import copy
import unittest
import probe


class EvidenceTests(unittest.TestCase):
    def case(self):
        return {'enabled': True, 'thread_id': 'thread', 'turn_id': 'turn',
                'provider_requests': [{'model': probe.SELECTED_MODEL, 'effort': 'high',
                    'function_tools': [probe.SENTINEL_TOOL]}],
                'events': [{'method':'turn/started','thread_id':'thread','turn_id':'turn'},
                    {'method':'turn/completed','thread_id':'thread','turn_id':'turn','status':'completed'}],
                'output_correlated':True,'invalid_effort_rejected_before_provider':True,
                'cleanup_complete':True,'provider_rejected_connections':0,'provider_rejected_requests':0}

    def test_positive_contract(self):
        self.assertTrue(probe.verdict(self.case()))

    def test_missing_canary_cannot_be_green(self):
        case = self.case()
        case['provider_requests'][0]['function_tools'] = []
        self.assertFalse(probe.verdict(case))

    def test_cross_turn_completion_cannot_be_green(self):
        case = self.case()
        case['events'][1]['turn_id'] = 'other-turn'
        self.assertFalse(probe.verdict(case))

    def test_extra_physical_request_cannot_be_green(self):
        case = self.case()
        case['provider_requests'].append(copy.deepcopy(case['provider_requests'][0]))
        self.assertFalse(probe.verdict(case))

    def test_cleanup_and_negative_control_required(self):
        for field in ('cleanup_complete','invalid_effort_rejected_before_provider','output_correlated'):
            with self.subTest(field=field):
                case = self.case(); case[field] = False
                self.assertFalse(probe.verdict(case))

    def test_wrong_model_or_effort_cannot_be_green(self):
        for field in ('model','effort'):
            with self.subTest(field=field):
                case = self.case(); case['provider_requests'][0][field] = 'wrong'
                self.assertFalse(probe.verdict(case))

    def test_disabled_means_baseline_and_no_canary(self):
        case = self.case(); case['enabled'] = False
        case['provider_requests'] = [{'model':probe.BASE_MODEL,'effort':'low','function_tools':[]}]
        self.assertTrue(probe.verdict(case))
        case['provider_requests'][0]['function_tools'] = [probe.SENTINEL_TOOL]
        self.assertFalse(probe.verdict(case))

    def test_projection_omits_prompt_and_key_and_reads_namespaces(self):
        actual = probe.request_projection({'model':'m','reasoning':{'effort':'high'},
            'input':'private prompt','api_key':'private key','tools':[
                {'type':'namespace','name':'n','tools':[{'type':'function','name':'f','description':'private'}]}]})
        self.assertEqual(actual, {'model':'m','effort':'high','function_tools':['f'],'tool_types':['namespace']})


if __name__ == '__main__':
    unittest.main()
