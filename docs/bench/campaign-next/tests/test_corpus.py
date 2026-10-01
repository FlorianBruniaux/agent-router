import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CORPUS = ROOT / "pilot" / "corpus"


class CandidateCorpusTests(unittest.TestCase):
    def test_each_hidden_suite_fails_on_base_and_passes_reference_solution(self):
        for task_dir in sorted((CORPUS / "tasks").iterdir()):
            with self.subTest(task=task_dir.name), tempfile.TemporaryDirectory() as tmp:
                task = json.loads((task_dir / "meta.json").read_text())
                workspace = Path(tmp) / "workspace"
                shutil.copytree(CORPUS / "base", workspace)
                shutil.copy(task_dir / "hidden_test.py", workspace / "tests" / "test_hidden_acceptance.py")
                command = [sys.executable, "-m", "unittest", "tests.test_hidden_acceptance"]
                base = subprocess.run(command, cwd=workspace, capture_output=True, text=True)
                self.assertNotEqual(base.returncode, 0, base.stdout + base.stderr)
                solution = CORPUS / "solutions" / f"{task['id']}.py"
                shutil.copy(solution, workspace / task["target"])
                solved = subprocess.run(command, cwd=workspace, capture_output=True, text=True)
                self.assertEqual(solved.returncode, 0, solved.stdout + solved.stderr)


if __name__ == "__main__":
    unittest.main()
