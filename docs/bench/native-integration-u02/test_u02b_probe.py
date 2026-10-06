"""Offline checks for the isolated discovery projection; never trusts a hook."""
import unittest
import u02b_probe as p

class HookProjectionTests(unittest.TestCase):
    def test_untrusted_hook_cannot_count_as_invoked_or_applied(self):
        response={'data':[{'cwd':'/private/path','errors':[],'warnings':[], 'hooks':[
            {'eventName':'userPromptSubmit','trustStatus':'untrusted','enabled':True,
             'currentHash':'sha256:fixture','source':'user','sourcePath':'/private/config',
             'command':'private command'}]}]}
        result=p.project_hooks(response)
        self.assertEqual(result,{'candidate_count':1,'error_count':0,'warning_count':0,
            'candidates':[{'event':'userPromptSubmit','enabled':True,'trust_status':'untrusted',
                'current_hash':'sha256:fixture','source':'user'}]})
        self.assertNotIn('/private',str(result))

if __name__=='__main__':unittest.main()
