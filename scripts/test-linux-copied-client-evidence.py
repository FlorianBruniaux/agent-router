#!/usr/bin/env python3
"""Synthetic negative receipt witnesses; no native execution or model calls."""
import importlib.util
import hashlib
import json
from pathlib import Path
import unittest
ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('copied_linux', ROOT/'docs/bench/linux-copied-client-2026-10-11/run.py')
r = importlib.util.module_from_spec(spec); spec.loader.exec_module(r)


class Receipts(unittest.TestCase):
    def setUp(self):
        # Deliberately synthetic data. This fixture cannot qualify a real client.
        self.row = {'diagnostic_only':True,'f12a':'NO_GO','case':'normal','address_space_bytes':8589934592,
            'protected_identities_after':'unchanged','controller_still_owns_copies':True,
            'namespace_cleanup':'PENDING_EXTERNAL_OWNER','native_rpc':'initialize-and-thread-start-only',
            'provider_turns':0,'broker_connections':3,'other_listener_connections':1,
            'canary':{'memory':'8GiB','protected_assets':6,'broker':'exact-port','detached_pid':99},
            'copy_hashes':['a'*64]*6,'helper_exit_code':0,'lease':'router-runtime-lease-v1:group-clean'}
        self.record = {'image':r.IMAGE,'model_calls':0,'provisional':self.row,'stop_reason':None,'diagnostic_only':True,'f12a':'NO_GO',
            'memory_bytes':8589934592,'namespace_exit_code':137,'namespace_stopped':True,
            'container_absent':True,'volume_removed':True,'heartbeat_before_teardown':[100,120],
            'heartbeat_after_teardown':[125,125,125]}

    def reject(self):
        with self.assertRaises((AssertionError,KeyError)):
            r.validate_complete(self.record)

    def test_complete_synthetic_contract(self):
        r.validate_complete(self.record)

    def test_provisional_receipt_cannot_release_namespace(self):
        self.record.pop('namespace_stopped'); self.reject()

    def test_retained_activity_not_observed(self):
        self.record['heartbeat_before_teardown']=[120,120]; self.reject()

    def test_activity_continues_after_stop(self):
        self.record['heartbeat_after_teardown']=[125,125,126]; self.reject()

    def test_volume_cleanup_unknown(self):
        self.record['volume_removed']=None; self.reject()

    def test_wrong_memory_bound(self):
        self.row['address_space_bytes']=None; self.reject()

    def test_protected_asset_identity_changed(self):
        self.row['protected_identities_after']='changed'; self.reject()

    def test_owner_released_before_teardown(self):
        self.row['controller_still_owns_copies']=False; self.reject()

    def test_other_endpoint_reached(self):
        self.row['other_listener_connections']=2; self.reject()

    def test_native_promotion_refused(self):
        self.record['f12a']='GO'; self.reject()

    def test_model_turn_refused(self):
        self.row['provider_turns']=1; self.reject()

    def test_boolean_exit_is_not_success(self):
        self.row['helper_exit_code']=False; self.reject()

    def test_helper_death_cannot_claim_group_cleanup(self):
        self.row['case']='helper-kill'; self.row['helper_exit_code']=None; self.reject()

    def test_helper_death_active_lease_can_be_observed(self):
        self.row['case']='helper-kill'; self.row['helper_exit_code']=None
        self.row['lease']='router-runtime-lease-v1:active'
        r.validate_complete(self.record)

    def test_helper_death_pending_lease_is_not_a_started_worker(self):
        self.row['case']='helper-kill'; self.row['helper_exit_code']=None
        self.row['lease']='router-runtime-lease-v1:pending'; self.reject()

    def test_capture_failure_is_not_cleanup_success(self):
        self.record['stop_reason']='timeout'; self.reject()

    def test_other_runtime_image(self):
        self.record['image']='sha256:'+'b'*64; self.reject()

    def test_collector_model_call(self):
        self.record['model_calls']=1; self.reject()

    def test_non_digest_asset_pin(self):
        self.row['copy_hashes'][0]='z'*64; self.reject()

    def test_missing_asset_pin(self):
        self.row['copy_hashes'].pop(); self.reject()

    def test_retained_campaign_and_executed_fixture_binding(self):
        evidence = json.loads((ROOT/'docs/bench/linux-copied-client-2026-10-11/evidence.json').read_text())
        summary = evidence['campaign']
        self.assertEqual([x['case'] for x in summary['cases']], list(r.CASES))
        self.assertEqual(evidence['f12a'], 'NO_GO')
        for row in summary['cases']:
            r.validate_complete(row)
            self.assertEqual(row['provisional']['copy_hashes'][:4],
                [summary['binaries_sha256'][x] for x in ('client','native','helper','wrapper')])
        # Historical product sources belong to PR171's tested head, not HEAD.
        # Freeze their published hashes and cross-check the actual final build inputs.
        # This also works from a source archive without historical Git objects.
        frozen = {
            'crates/router-runtime/src/finite_client.rs': '403ad0c617738ca9905c35c3e68cfa47895c7ec0ac3f0d470634116230297319',
            'crates/router-runtime/src/finite_client/linux_qualification.rs': 'c1b63f78b449fd0e3e11006e9afb9172b68afcf07575d028a73194203d4aa378',
            'crates/router-runtime/examples/finite_client_supervision.rs': '2e990275deecbf10f084f72cafdc36df38dd3e07191df3136fb6b972887716d4',
        }
        self.assertEqual(evidence['source_inputs']['final_controller']['source_sha256'], frozen)
        for f, expected in evidence['source_sha256'].items():
            actual = frozen[f] if f in frozen else hashlib.sha256((ROOT/f).read_bytes()).hexdigest()
            self.assertEqual(actual, expected, f)
        self.assertEqual(summary['inside_sha256'], evidence['source_sha256']['docs/bench/linux-copied-client-2026-10-11/inside.py'])

    def test_container_limits_and_no_network(self):
        for flag in ('--network=none','--read-only','--cap-drop=ALL','--security-opt=no-new-privileges','--memory=8589934592','--memory-swap=8589934592','--pull=never'):
            self.assertIn(flag,r.settings())


if __name__ == '__main__':
    unittest.main(verbosity=2)
