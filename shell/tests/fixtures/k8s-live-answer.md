**Kubernetes (k8s) runs containerized apps across many machines and keeps them matching the state you declare.**

- You describe the desired state, such as "3 replicas of this service," in YAML manifests.
- The control plane (API server, scheduler, controller manager, and etcd) stores that state and assigns containers to worker nodes.
- A kubelet on each node starts the containers and reports their health back.
- Controllers constantly compare actual state with desired state and restart, reschedule, or scale to close any gap.