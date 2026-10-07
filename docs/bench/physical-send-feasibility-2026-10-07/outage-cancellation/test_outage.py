import copy
import unittest
from outage import qualify

class QualificationTests(unittest.TestCase):
    def valid(self):
        return {'rpc': [
            {'id':2,'result':{'thread':{'id':'t'}}},
            {'id':3,'result':{'turn':{'id':'u'}}},
            {'method':'error','params':{'threadId':'t','turnId':'u','willRetry':True}},
            {'id':9001,'result':{}},
            {'method':'turn/completed','params':{'threadId':'t','turn':{'id':'u','status':'interrupted'}}}],
            'sent':[{'after_received':1,'message':{'id':3,'method':'turn/start','params':{'threadId':'t'}}},
                {'after_received':3,'message':{'id':9001,'method':'turn/interrupt','params':{'threadId':'t','turnId':'u'}}}],
            'native':{'failure':None,'cleanup_complete':True,'process_exit_code':0,'process_group_gone':True,
                'output_complete':True,'turn_correlated':True,'broker_http_requests':0,'provider_http_requests':0,
                'broker_receives':0,'provider_receives':0,'reserved':0,'decisions':[],
                'provider_body_hashes':[],'broker_body_hashes':[],'provider_rejected_connections':0,
                'broker_rejected_connections':0,'turn_status':'interrupted','final_output_observed':False},
            'provider_accepted_connections':0,'provider_rejected_connections':0,
            'provider_handlers_stopped':True,'broker_handlers_stopped':True,
            'listeners_closed':True,'pin_unchanged':True,'native_launch_count':1,
            'listener_checks':[{'fd_closed':True,'connect_errno':61},{'fd_closed':True,'connect_errno':61}]}

    def test_complete_interruption_is_accepted(self):
        self.assertTrue(qualify(self.valid()))

    def test_ack_without_terminal_is_not_cleanup(self):
        d=self.valid();d['rpc'].pop()
        self.assertFalse(qualify(d))

    def test_terminal_without_physical_cleanup_fails(self):
        for key in ('cleanup_complete','process_group_gone','output_complete'):
            d=self.valid();d['native'][key]=False
            with self.subTest(key=key): self.assertFalse(qualify(d))

    def test_mismatched_identity_or_uncommanded_terminal_fails(self):
        for field in ('threadId','turnId'):
            d=self.valid();d['sent'][1]['message']['params'][field]='wrong'
            with self.subTest(field=field): self.assertFalse(qualify(d))
        d=self.valid();d['sent'][1]['after_received']=2
        self.assertFalse(qualify(d))

    def test_provider_connection_or_http_or_relaunch_fails(self):
        d=self.valid();d['provider_accepted_connections']=1
        self.assertFalse(qualify(d))
        d=self.valid();d['native']['provider_http_requests']=1
        self.assertFalse(qualify(d))
        d=self.valid();d['sent'].append(copy.deepcopy(d['sent'][0]))
        self.assertFalse(qualify(d))

    def test_disagreeing_raw_and_aggregate_evidence_fails(self):
        for field,value in (('provider_body_hashes',['unexpected']),('provider_rejected_connections',1),('turn_status','completed')):
            d=self.valid();d['native'][field]=value
            with self.subTest(field=field):self.assertFalse(qualify(d))
        d=self.valid();d['listener_checks'][0]['fd_closed']=False
        self.assertFalse(qualify(d))

class InterruptSelectionTests(unittest.TestCase):
    def test_only_current_correlated_retry_selects_interrupt(self):
        from outage import make_interrupt
        message={'method':'error','params':{'willRetry':True,'threadId':'t','turnId':'u'}}
        self.assertEqual(make_interrupt(message,'t','u'),{'id':9001,'method':'turn/interrupt','params':{'threadId':'t','turnId':'u'}})
        for field,value in (('threadId','other'),('turnId','other'),('willRetry',False)):
            wrong=copy.deepcopy(message);wrong['params'][field]=value
            with self.subTest(field=field):self.assertIsNone(make_interrupt(wrong,'t','u'))

    def test_missing_identity_or_malformed_event_cannot_interrupt(self):
        from outage import make_interrupt
        for message in ({}, {'method':'error','params':None}, {'method':'error','params':{'willRetry':True}}):
            with self.subTest(message=message):self.assertIsNone(make_interrupt(message,'t','u'))
        self.assertIsNone(make_interrupt({'method':'error','params':{'willRetry':True,'threadId':None,'turnId':None}},None,None))

if __name__=='__main__':unittest.main()
