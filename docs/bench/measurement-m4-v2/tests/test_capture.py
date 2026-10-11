import copy,json,unittest,pathlib,sys
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
import capture

class CaptureTests(unittest.TestCase):
    def rows(self):
        u=dict(zip(capture.FIELDS,(100,80,0,20,5,120)))
        return [{'type':'session_meta','payload':{'id':'id','history_base':None,'source':{'subagent':{'thread_spawn':{'parent_thread_id':'parent','agent_path':'/root/test'}}}}},
                {'type':'turn_context','payload':{'model':'gpt-6-luna','effort':'high'}},
                {'type':'event_msg','ordinal':1,'payload':{'type':'task_started'}},
                {'type':'token_usage_record','ordinal':2,'payload':{'thread_id':'id','response_id':'response-1','usage':u,'thread_token_usage':u}},
                {'type':'event_msg','ordinal':3,'payload':{'type':'token_count','info':{'total_token_usage':u}}},
                {'type':'event_msg','ordinal':4,'payload':{'type':'task_complete'}}]
    def parse(self,rows):return capture.parse(('\n'.join(json.dumps(r) for r in rows)+'\n').encode(),'id','parent','/root/test','gpt-6-luna','high')
    def test_complete(self):
        r=self.parse(self.rows());self.assertTrue(r['terminal']);self.assertEqual(r['cumulative']['total_tokens'],120)
    def test_absent_history_field_does_not_invent_fresh_context(self):
        rows=self.rows();del rows[0]['payload']['history_base']
        result=self.parse(rows)
        self.assertEqual(result['history_origin'],'not_declared')
        self.assertEqual(result['cumulative']['total_tokens'],120)
    def test_unexpected_metadata_is_not_exported(self):
        rows=self.rows();rows[0]['payload']['cli_version']='PRIVATE_CANARY'
        rows[3]['timestamp']='PRIVATE_CANARY'
        result=self.parse(rows)
        self.assertNotIn('PRIVATE_CANARY',json.dumps(result))
        self.assertIsNone(result['runtime_cli_version'])
        self.assertIsNone(result['response_records'][0]['timestamp'])
    def test_identical_response_not_counted_twice(self):
        r=self.rows();r.insert(4,copy.deepcopy(r[3]));self.assertEqual(self.parse(r)['response_record_count'],1)
    def test_invalid_evidence_refused(self):
        for mutation in ('wrong_parent','wrong_model','missing_cache','sum_mismatch','event_mismatch','duplicate_conflict','wrong_thread','inherited','partial_bool'):
            with self.subTest(mutation=mutation):
                r=self.rows();q=r[3]['payload'];q['usage']=dict(q['usage']);q['thread_token_usage']=dict(q['thread_token_usage'])
                if mutation=='wrong_parent':r[0]['payload']['source']['subagent']['thread_spawn']['parent_thread_id']='other'
                elif mutation=='wrong_model':r[1]['payload']['model']='gpt-6.1-sol'
                elif mutation=='missing_cache':del q['usage']['cache_write_input_tokens']
                elif mutation=='sum_mismatch':q['thread_token_usage']['output_tokens']+=1;q['thread_token_usage']['total_tokens']+=1
                elif mutation=='event_mismatch':r[4]['payload']['info']['total_token_usage']={k:0 for k in capture.FIELDS}
                elif mutation=='duplicate_conflict':r.insert(4,copy.deepcopy(r[3]));r[4]['payload']['response_id']='response-2'
                elif mutation=='wrong_thread':q['thread_id']='other'
                elif mutation=='inherited':r[0]['payload']['history_base']='other'
                elif mutation=='partial_bool':q['usage']['cache_write_input_tokens']=False
                with self.assertRaises(ValueError):self.parse(r)
    def test_missing_usage_is_not_zero(self):
        with self.assertRaisesRegex(ValueError,'usage_not_yet_observed'):self.parse(self.rows()[:3])
    def test_partial_record_refused(self):
        with self.assertRaisesRegex(ValueError,'pending_partial_record'):capture.parse(b'{}','id','parent','/root/test','gpt-6-luna','high')
    def test_conflicting_duplicate_refused(self):
        r=self.rows();r.insert(4,copy.deepcopy(r[3]));r[4]['payload']['extra']='tampered'
        with self.assertRaisesRegex(ValueError,'conflicting_response_snapshot'):self.parse(r)

if __name__=='__main__':unittest.main()
