---
title: "fixture — valid flowchart"
models: "fixture diagram for diagram-linter integration tests"
source_of_truth: diagram
last_verified: bootstrap 2026-05-23
diagram_type: flowchart
---

# Valid fixture

```mermaid
flowchart TD
  A[Start] --> B{Decision}
  B -->|yes| C[End]
  B -->|no| D[Other]
```
