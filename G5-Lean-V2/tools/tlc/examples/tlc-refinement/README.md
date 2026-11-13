# A small, executable refinement and evidence example

Read [the intent](intent.md) and [the correspondence and variant analysis](bindings.md).
`Intent.tla` is L1; `Implementation.tla` is L2; `store_fixture.py` is an L3 teaching
fixture. The bounded check verifies an explicit abstraction map and next-state
correspondence. It does not formally verify Python or establish unbounded behavior.

Use the qualified TLC release, **2.19 of 08 August 2024 (revision 5a47802)**, and
Java 21. Supply paths to tools already installed; this example downloads nothing.
From this directory:

```sh
python3 run_example.py --java /path/to/java --jar /path/to/tla2tools.jar --output /new/evidence/directory
```

The driver creates fourteen separate manifests, raw TLC logs, results and helper
receipts. It refuses an existing output directory. Eight cases should match their
declared outcome, including expected violations and witnesses; six must be
rejected, including a missing path, wrong property label, parse/config errors,
insufficient JVM heap and timeout. A successful driver exit means these acceptance
and rejection expectations were met. **It does not mean fourteen safety passes.**

Run the tests from the repository root, supplying explicit installed tool paths:

```sh
TLC_JAVA=/path/to/java TLC_JAR=/path/to/tla2tools.jar python3 -m unittest discover -s tools/tlc/scripts -p test_tlc_adapter.py -v
```

Without both environment variables, real-tool qualification tests are explicitly
skipped. Tests also truncate actual output, remove counterexample states, change
the property/version/exit status, compare a path with the fixture, and check the
adapter through `model_evidence.py`, including a stale binding. They do not treat
a substring or a process exit alone as a model result.

For other models, call `../../scripts/tlc_adapter.py --help`. It accepts explicit
Java, jar, model root, module, config, expected outcome and property, plus timeout,
heap size and an optional exact action diagnostic label. Place that command in
a `model_evidence.py` manifest and declare the adapter, Java executable, jar,
all formal dependencies/configs, governing intent, binding and relevant runtime
sources. Standalone adapter output lacks those applicability fingerprints.

The adapter deliberately supports a narrow configuration/message subset. It
rejects unknown versions, warnings, parser/resource failures, incomplete output,
ambiguous property selection, constraints and symmetry rather than guessing.
See its module documentation for limits. Extend it only with real-output tests
for the additional configuration and TLC outcome being supported.

The checked type-only and stuck variants explain why a green result can be
uninformative: a weak statement can miss a wrong write, and a model missing a
publish transition can satisfy safety. Witnesses and executable correspondence
distinguish those failures from a correct model. The direct-commit variant shows
an internal simplification that preserves this narrow intent, subject to the
integration assumptions recorded in the binding.
