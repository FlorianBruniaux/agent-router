# SPDX-FileCopyrightText: 2026 Florian Bruniaux
# SPDX-License-Identifier: Apache-2.0
"""Exercise the interposer on disposable bytes, not the native product."""
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time
import unittest

class Interposer(unittest.TestCase):
    def test_boundaries_and_controls(self):
        for stage in ('before','after','partial','unarmed','wrong-target'):
            with self.subTest(stage=stage), tempfile.TemporaryDirectory(dir='/task') as directory:
                root = Path(directory)
                target, arm = root/'target', root/'armed'
                target.write_bytes(b'before')
                if stage != 'unarmed':
                    arm.write_bytes(b'armed')
                env = dict(LD_PRELOAD='/task/built/cut.so', AR_CUT_STAGE='before' if stage in ('unarmed','wrong-target') else stage,
                    AR_CUT_TARGET=str(target if stage != 'wrong-target' else root/'different'), AR_CUT_ARM=str(arm))
                child = subprocess.Popen(['/task/programs/agent-router', str(target)], env=env)
                try:
                    if stage in ('before','after','partial'):
                        deadline=time.monotonic()+5
                        while True:
                            pid,status=os.waitpid(child.pid,os.WNOHANG|os.WUNTRACED)
                            if pid:
                                self.assertTrue(os.WIFSTOPPED(status))
                                self.assertEqual(os.WSTOPSIG(status),signal.SIGSTOP)
                                break
                            self.assertLess(time.monotonic(),deadline)
                            time.sleep(.01)
                        self.assertEqual(target.read_bytes(),{'before':b'before','after':b'after!','partial':b'aftore'}[stage])
                        child.kill()
                        self.assertEqual(child.wait(timeout=5),-signal.SIGKILL)
                    else:
                        self.assertEqual(child.wait(timeout=5),0)
                        self.assertEqual(target.read_bytes(),b'after!')
                finally:
                    if child.poll() is None:
                        child.kill()
                    child.wait()

if __name__=='__main__':
    unittest.main()
