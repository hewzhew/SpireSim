# Third-Party Notices

## Stable-Baselines3

`learning/src/sts_learning/run_rollout.py` adapts the reverse generalized
advantage-estimation and lambda-return recurrence from Stable-Baselines3
v2.9.0, commit `8908708f10c8ff29759c67f55c8acb56cab27463`, file
`stable_baselines3/common/buffers.py`. The local implementation is rewritten
around complete typed attempts, ragged candidates, undiscounted action time,
terminal-only episodes, policy sums averaged across attempts, and auxiliary
value errors averaged within and across attempts.

Upstream project: <https://github.com/DLR-RM/stable-baselines3>

The MIT License

Copyright (c) 2019 Antonin Raffin

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## Replay presentation

`tools/presentation` uses [i18next 26.5.4](https://github.com/i18next/i18next) for message formatting and language fallback. Its replay layout adapts the [Tabler](https://github.com/tabler/tabler) vertical template and uses Tabler 1.4.0 styles. Rendered icon symbols come from [Tabler Icons 3.35.0](https://github.com/tabler/tabler-icons). Offline builds retain the full component licenses alongside the bundled code and assets.

The following components use the MIT License:

- i18next: Copyright (c) 2011-present i18next.
- Tabler: Copyright (c) 2018-2026 The Tabler Authors; CSS also credits Copyright 2018-2025 codecalm.net Paweł Kuna.
- Tabler Icons: Copyright (c) 2020-2025 Paweł Kuna.

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
