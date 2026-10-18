"""Offline capsule verification. Never activate subscription mode or a real campaign."""
import hashlib,importlib.util,json,pathlib,sys,unittest
root=pathlib.Path(__file__).resolve().parent
sys.path.insert(0,str(root/'controller'))
files=['test_subscription_gateway.py','relay-controller-test.py','relay-controller-results-test.py',
       'native-catalog-adapter/test_native_adapter.py','relay-controller-session-test.py',
       'relay-controller-session-driver-test.py','relay-controller-results-v2-test.py']
suite=unittest.TestSuite();loader=unittest.TestLoader()
for index,name in enumerate(files):
 spec=importlib.util.spec_from_file_location('coupling_qualification_'+str(index),root/'controller'/name)
 module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
 suite.addTests(loader.loadTestsFromModule(module))
with (root/'offline-tests.log').open('w') as output:
 result=unittest.TextTestRunner(stream=output,verbosity=2).run(suite)
report={'tests':result.testsRun,'failures':len(result.failures),'errors':len(result.errors),
        'successful':result.wasSuccessful(),'model_calls_started':0,'actual_provider_requests':0,
        'synthetic_approval_and_account_inputs_only':True,'real_campaign_created':False,
        'scope':'host composition + versioned results with coded responses; not native Runtime evidence'}
(root/'offline-tests.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report))
raise SystemExit(not result.wasSuccessful())
