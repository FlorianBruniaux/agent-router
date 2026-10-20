import importlib.util,json,pathlib,unittest

ROOT=pathlib.Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('mac_e2e',ROOT/'relay-controller-macos-e2e.py')
driver=importlib.util.module_from_spec(spec);spec.loader.exec_module(driver)

class BridgeTests(unittest.TestCase):
    def test_recipe_placeholders_preserve_shell_positional_arguments(self):
        value=driver.resolve_argv(['docker','exec','@CONFIG_SHA@','@CONTROLLER_SHA@','exec fixture "$@"'],'c'*64,'d'*64)
        self.assertEqual(value[-1],'exec fixture "$@"')
        self.assertEqual(value[2:4],['c'*64,'d'*64])
        with self.assertRaises(ValueError):driver.resolve_argv(['@UNKNOWN@'],'c'*64,'d'*64)

    def test_only_ordered_runtime_control_prefixes_enter_protocol(self):
        parser=driver.ControlParser()
        self.assertIsNone(parser.accept(b'worker-looking libtest log\n'))
        self.assertEqual(parser.accept(b'AR_RELAY_PREPARE {"qualification_only":true}\n'),('prepare',{'qualification_only':True}))
        self.assertEqual(parser.accept(b'AR_RELAY_BINDING {"run_id":"fixture"}\n'),('binding',{'run_id':'fixture'}))
        self.assertEqual(parser.accept(b'AR_RELAY_V1 {"schema_version":1}\n'),('frame',{'schema_version':1}))
        self.assertEqual(parser.log_lines,1)

    def test_out_of_order_duplicate_overflow_and_duplicate_json_refuse(self):
        for line in [b'AR_RELAY_BINDING {}\n',b'AR_RELAY_V1 {}\n',b'x'*300001,b'AR_RELAY_PREPARE {"a":1,"a":2}\n']:
            with self.subTest(length=len(line)),self.assertRaises(ValueError):driver.ControlParser().accept(line)
        parser=driver.ControlParser();parser.accept(b'AR_RELAY_PREPARE {}\n')
        with self.assertRaises(ValueError):parser.accept(b'AR_RELAY_PREPARE {}\n')

    def test_descriptor_is_public_fixed_and_remote_path_never_selected(self):
        descriptor=driver.public_descriptor('/private/tmp/local-root')
        remote=dict(descriptor,evidence_directory='/task/linux-root',output_content='Public generated Rust fixture.\n')
        local=driver.relocate_descriptor(remote,'/private/tmp/local-root')
        self.assertEqual(local['evidence_directory'],'/private/tmp/local-root')
        self.assertEqual(local['output_content'],remote['output_content'])
        for change in [{'account_sha256':'a'*64},{'qualification_only':False},{'extra':'value'},{'output_content':'x'*65537}]:
            with self.subTest(change=list(change)),self.assertRaises(ValueError):driver.relocate_descriptor(dict(remote,**change),'/private/tmp/local-root')

if __name__=='__main__':unittest.main()
