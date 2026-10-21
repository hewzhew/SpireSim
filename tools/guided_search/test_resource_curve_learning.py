import pytest
import torch

from resource_curve_learning import ResourceCurve, labels


def model():
    names = ["terminal_hp_ceiling"] + [f"hand/{i}/present" for i in range(10)]
    net = ResourceCurve(names)
    with torch.no_grad():
        for p in net.parameters():
            p.zero_()
        net.source.output.bias[0] = 1.2
        net.slope.bias[0] = torch.log(torch.expm1(torch.tensor(3.)))
    x = torch.tensor([[.8] + [0.] * 10])
    net.requires_grad_(False)
    return net, x, x[:, 0]


def test_goals_are_monotone_integer_bounded_and_loss_is_not_zero_hp_win():
    net, x, ceiling = model()
    probabilities = [float(net.probability(x, ceiling, goal)) for goal in range(76)]
    assert all(a >= b for a, b in zip(probabilities, probabilities[1:]))
    assert probabilities[60] > 0 and probabilities[61] == 0
    returns = torch.tensor([-1., 0., 19./75.])
    assert labels(returns, 0).tolist() == [0., 1., 1.]
    assert labels(returns, 19).tolist() == [0., 0., 1.]
    assert labels(returns, 20).tolist() == [0., 0., 0.]
    with pytest.raises(ValueError, match="integer"):
        net.probability(x, ceiling, .5)
    with pytest.raises(ValueError, match="integer"):
        net.probability(x, ceiling, -1)


def test_integrating_goals_matches_expected_return_without_choosing_a_threshold():
    net, x, ceiling = model()
    q = torch.linspace(0, .8, 10001, dtype=torch.float64)
    tail = torch.sigmoid(1.2 - 3 * q)
    expected = torch.sigmoid(torch.tensor(1.2, dtype=torch.float64)) - 1 + torch.trapezoid(tail, q)
    assert float(net.value(x, ceiling)) == pytest.approx(float(expected), abs=1e-6)
    exported = net.export(["terminal_hp_ceiling"] + [f"hand/{i}/present" for i in range(10)])
    assert exported["resource_curve"]["slope_bias"] == pytest.approx(float(net.slope.bias[0]))


def test_saturated_logits_and_nearly_flat_curves_remain_finite_and_bounded():
    net, x, ceiling = model()
    for intercept, raw_slope in ((1000., 1000.), (1000., -1000.), (-1000., 1000.), (1., -30.)):
        with torch.no_grad():
            net.source.output.bias[0] = intercept
            net.slope.bias[0] = raw_slope
        value = net.value(x, ceiling)
        assert torch.isfinite(value).all()
        assert -1 <= float(value) <= float(ceiling)
