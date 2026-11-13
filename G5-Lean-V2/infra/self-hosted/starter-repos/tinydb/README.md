# TinyDB Starter Repository

This starter repository is created in new G5 self-hosted installations as a
small, inspectable codebase for validating ontology workflows.

It contains:

- `tinydb/` Python source files.
- `.semlift/specs/` SemLift ontology files that were already uplifted from the
  source.

The self-hosted bootstrap commits these files to the local TinyDB repository,
creates the default `Base` workspace branch at that commit, and seeds the
matching ontology nodes as synced and uplifted.
