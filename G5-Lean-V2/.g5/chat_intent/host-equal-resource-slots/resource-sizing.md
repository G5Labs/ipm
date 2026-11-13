# Hardware-derived equal workspace VM slots

The user reported a QA c3-standard-22 host configured for six microVMs where the workspace received approximately 3.8 GiB RAM and two vCPUs. The user stated: “the expectation is that these 6 microVM should equally share the host resource as the v9 design said.”

The agent proposed restoring host-measured equal slots, reserving host RAM, publishing resolved sizes before readiness, aligning API boot specifications with the host, covering provisioning/rotation/retry/self-hosted bootstrap, displaying actual sizes, and keeping sizes stable while VMs are running. The user selected “I recommend restoring hardware-derived equal slots:” and replied “agree”, then explicitly invoked build-to-ship to implement and merge the agreed repair.

Integer vCPU division and the previous host RAM reserve yield three vCPUs and approximately thirteen GiB per VM on the measured QA host with six slots. This is an expected sizing result, not an observation of a deployed fix.
