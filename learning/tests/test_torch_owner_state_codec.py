from __future__ import annotations

import importlib.util
import unittest


_TORCH_AVAILABLE = importlib.util.find_spec("torch") is not None

if _TORCH_AVAILABLE:
    import torch

    from sts_learning._torch_owner_state_codec import (
        TorchOwnerStateError,
        decode_owner_state,
        encode_owner_state,
    )
    from sts_learning.torch_resume import (
        TorchResumeStateError,
        encode_generator_state,
        encode_optimizer_state,
        encode_shadow_model_state,
        hydrate_fresh_optimizer,
        materialize_generator_state,
        materialize_shadow_model_state,
    )


@unittest.skipUnless(_TORCH_AVAILABLE, "optional PyTorch dependency is not installed")
class TorchOwnerStateCodecTests(unittest.TestCase):
    def test_shadow_model_round_trip_is_independent_from_optimizer(self) -> None:
        model = torch.nn.Linear(3, 2)
        payload = encode_shadow_model_state(model, max_bytes=1024 * 1024)
        restored = materialize_shadow_model_state(
            payload,
            lambda: torch.nn.Linear(3, 2),
            max_bytes=1024 * 1024,
        )
        for left, right in zip(model.parameters(), restored.parameters(), strict=True):
            self.assertTrue(torch.equal(left, right))

    def test_adam_state_round_trip_preserves_the_next_optimizer_update(self) -> None:
        torch.manual_seed(17)
        left = torch.nn.Linear(3, 2)
        left_optimizer = torch.optim.Adam(left.parameters(), lr=0.003)
        _optimizer_step(left, left_optimizer)

        payload = encode_optimizer_state(left_optimizer, max_bytes=1024 * 1024)
        self.assertEqual(
            payload,
            encode_optimizer_state(left_optimizer, max_bytes=1024 * 1024),
        )

        right = torch.nn.Linear(3, 2)
        right.load_state_dict(left.state_dict())
        right_optimizer = torch.optim.Adam(right.parameters(), lr=0.003)
        hydrate_fresh_optimizer(
            right_optimizer,
            payload,
            max_bytes=1024 * 1024,
        )

        _optimizer_step(left, left_optimizer)
        _optimizer_step(right, right_optimizer)
        for left_parameter, right_parameter in zip(
            left.parameters(), right.parameters(), strict=True
        ):
            self.assertTrue(torch.equal(left_parameter, right_parameter))
        self.assertEqual(
            encode_optimizer_state(left_optimizer, max_bytes=1024 * 1024),
            encode_optimizer_state(right_optimizer, max_bytes=1024 * 1024),
        )

    def test_generator_state_round_trip_preserves_the_next_sample(self) -> None:
        left = torch.Generator().manual_seed(991)
        torch.rand(7, generator=left)
        payload = encode_generator_state(left, max_bytes=1024 * 1024)
        right = materialize_generator_state(
            payload,
            expected_device_type="cpu",
            max_bytes=1024 * 1024,
        )
        self.assertTrue(
            torch.equal(
                torch.rand(64, generator=left),
                torch.rand(64, generator=right),
            )
        )

    def test_optimizer_topology_and_component_kind_fail_before_hydration(self) -> None:
        model = torch.nn.Linear(3, 2)
        optimizer = torch.optim.Adam(model.parameters(), lr=0.003)
        _optimizer_step(model, optimizer)
        payload = encode_optimizer_state(optimizer, max_bytes=1024 * 1024)

        wrong_model = torch.nn.Sequential(
            torch.nn.Linear(3, 2),
            torch.nn.Linear(2, 1),
        )
        wrong_optimizer = torch.optim.Adam(wrong_model.parameters(), lr=0.003)
        with self.assertRaisesRegex(TorchResumeStateError, "topology"):
            hydrate_fresh_optimizer(
                wrong_optimizer,
                payload,
                max_bytes=1024 * 1024,
            )

        generator_payload = encode_generator_state(
            torch.Generator().manual_seed(5),
            max_bytes=1024 * 1024,
        )
        with self.assertRaisesRegex(TorchResumeStateError, "not optimizer"):
            hydrate_fresh_optimizer(
                torch.optim.Adam(model.parameters(), lr=0.003),
                generator_payload,
                max_bytes=1024 * 1024,
            )

    def test_corrupt_unbounded_and_executable_values_fail_closed(self) -> None:
        state = {
            "state": [
                None,
                True,
                3,
                0.25,
                b"bytes",
                1 << 63,
                (1 << 64) - 1,
            ]
        }
        payload = encode_owner_state(
            state,
            max_bytes=1024,
        )
        self.assertEqual(decode_owner_state(payload, max_bytes=1024), state)
        with self.assertRaisesRegex(TorchOwnerStateError, "trailing"):
            decode_owner_state(payload + b"\x00", max_bytes=1024)
        with self.assertRaisesRegex(TorchOwnerStateError, "byte limit"):
            decode_owner_state(payload, max_bytes=len(payload) - 1)
        with self.assertRaisesRegex(TorchOwnerStateError, "unsupported"):
            encode_owner_state({"callable": lambda: None}, max_bytes=1024)
        with self.assertRaisesRegex(TorchOwnerStateError, "finite"):
            encode_owner_state({"loss": float("nan")}, max_bytes=1024)
        with self.assertRaisesRegex(TorchOwnerStateError, "keys"):
            encode_owner_state({True: 1}, max_bytes=1024)


def _optimizer_step(
    model: torch.nn.Module,
    optimizer: torch.optim.Optimizer,
) -> None:
    optimizer.zero_grad(set_to_none=True)
    inputs = torch.tensor(
        [[0.5, -0.25, 0.75], [-0.5, 0.125, 0.25]],
        dtype=torch.float32,
    )
    target = torch.tensor([[0.3, -0.1], [-0.2, 0.4]], dtype=torch.float32)
    loss = torch.square(model(inputs) - target).mean()
    loss.backward()
    optimizer.step()


if __name__ == "__main__":
    unittest.main()
