# skyward-root

The KernelSU payloads the recovery carries: every aarch64 LKM from one
KernelSU release, `ksud`, and the licence they are distributed under.

The modules are KernelSU's own, taken from
[`tiann/KernelSU`](https://github.com/tiann/KernelSU) release `v3.3.1`.
KernelSU is GPL-3.0-or-later; the `LICENSE` beside them is its licence.

`scripts/fetch-kernelsu.sh` refreshes the folder from the newest release, or
from `SKYWARD_KSU_TAG`. A build uses the files committed here unless
`SKYWARD_KSU_LATEST=1` asks `build.sh` to fetch first, which keeps ordinary
builds reproducible and offline.

The KMI names are `android<release>-<kernel>`. A phone's own Android release
and kernel release pick the default; the KernelSU page lets another be
chosen.
