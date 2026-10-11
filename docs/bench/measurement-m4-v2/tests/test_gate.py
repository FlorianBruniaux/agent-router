import copy
import json
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import gate
import test_capture


class GateTests(unittest.TestCase):
    def setUp(self):
        self.manifest = {"assignments": [
            {"arm_id": "router", "agent_path": "/root/test", "model": "gpt-6-luna", "effort": "high", "allocation_tokens": 120},
            {"arm_id": "strongfixed", "agent_path": "/root/strong", "model": "gpt-6.1-sol", "effort": "medium", "allocation_tokens": 120},
            {"arm_id": "cheapfixed", "agent_path": "/root/cheap", "model": "gpt-6-luna", "effort": "medium", "allocation_tokens": 120}],
            "envelope_tokens": 360}
        self.state = {"status": "ready", "parent_thread_id": "parent", "records": []}

    def trace(self, mutation=None):
        rows = test_capture.CaptureTests().rows()
        if mutation:
            mutation(rows)
        return ("\n".join(json.dumps(r) for r in rows) + "\n").encode()

    def test_cannot_reserve_twice_while_counter_is_pending(self):
        gate.reserve(self.state, self.manifest)
        with self.assertRaisesRegex(ValueError, "running"):
            gate.reserve(self.state, self.manifest)

    def test_complete_exact_budget_can_admit_next_and_keeps_cost(self):
        gate.reserve(self.state, self.manifest)
        gate.finish(self.state, self.manifest, self.trace(), "id")
        self.assertEqual(self.state["records"][0]["capture"]["cumulative"]["total_tokens"], 120)
        self.assertEqual(gate.reserve(self.state, self.manifest)["arm_id"], "strongfixed")

    def test_overrun_latches_stop_without_truncating_known_tokens(self):
        self.manifest["assignments"][0]["allocation_tokens"] = 119
        gate.reserve(self.state, self.manifest)
        gate.finish(self.state, self.manifest, self.trace(), "id")
        self.assertEqual(self.state["status"], "stopped")
        self.assertEqual(self.state["records"][0]["capture"]["cumulative"]["total_tokens"], 120)
        with self.assertRaises(ValueError):
            gate.reserve(self.state, self.manifest)

    def test_missing_counters_latch_stop_and_do_not_become_zero(self):
        gate.reserve(self.state, self.manifest)
        gate.finish(self.state, self.manifest, self.trace(lambda r: r.pop(3)), "id")
        self.assertEqual(self.state["status"], "stopped")
        self.assertIsNone(self.state["records"][0].get("capture"))
        with self.assertRaises(ValueError):
            gate.reserve(self.state, self.manifest)

    def test_extra_tool_call_or_second_turn_stops_next_assignment(self):
        mutations = [
            lambda r: r.insert(3, {"type": "response_item", "payload": {"type": "function_call", "name": "exec"}}),
            lambda r: r.append({"type": "event_msg", "ordinal": 5, "payload": {"type": "task_started"}}),
            lambda r: r.append({"type": "turn_context", "payload": {"model": "other", "effort": "high"}}),
        ]
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                state = copy.deepcopy(self.state)
                gate.reserve(state, self.manifest)
                gate.finish(state, self.manifest, self.trace(mutation), "id")
                self.assertEqual(state["status"], "stopped")
                with self.assertRaises(ValueError):
                    gate.reserve(state, self.manifest)

    def test_all_assigned_rows_retained_without_accepting_them(self):
        for i, assignment in enumerate(self.manifest["assignments"]):
            gate.reserve(self.state, self.manifest)
            def bind(rows):
                rows[0]["payload"]["id"] = str(i)
                rows[0]["payload"]["source"]["subagent"]["thread_spawn"]["agent_path"] = assignment["agent_path"]
                rows[1]["payload"] = {k: assignment[k] for k in ("model", "effort")}
                rows[3]["payload"]["thread_id"] = str(i)
            gate.finish(self.state, self.manifest, self.trace(bind), str(i))
        self.assertEqual(self.state["status"], "complete")
        self.assertEqual(len(self.state["records"]), 3)
        self.assertTrue(all(r["acceptance"] is None for r in self.state["records"]))
        with self.assertRaises(ValueError):
            gate.reserve(self.state, self.manifest)


if __name__ == "__main__":
    unittest.main()
