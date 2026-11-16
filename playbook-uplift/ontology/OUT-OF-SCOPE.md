# Named exclusions (review finding 9)

Areas with documentation or IPM tests that this ontology does not specify. Each is excluded by name, not by omission.

| Area | Where documented | Why excluded now |
|---|---|---|
| `-env` environment files and install-time configuration | Main.cls#L73 (load/install `-env`), CHANGELOG #1260, tests EnvironmentConfig.cls | Not reached in this run; candidate module 10 |
| The `test` and `verify` phases' reporting (`-f`, `-output-file`, `TestReportFormat`) | Main.cls#L50-L52, tests TestOutputFormat.cls | Reporting, not package state; IPM tests run (T) but not specified |
| Validate and Document phases | Lifecycle/Base.cls | No separate behaviour statements |
| Install side of resource processors: CPF merge, SystemSetting, web/CSP application creation, InstallerClass, `<Defaults>` and `${var}` expansion | CHANGELOG #1274; tests CPFMerge.cls, SystemSetting.cls, InstallApplication.cls, ModuleDefaults | Only the uninstall side (module 05) and FileCopy/scopes (module 08) are specified |
| `load -create-lockfile` (load side), `install -latest`, `install -keywords`, `list-installed` options | Main.cls XData Commands | Lock-file writing is specified for install (module 04) only |
| Mirrors (all-member vs primary-only update steps) | VersionBase.cls | Community Edition refuses mirroring (O:nc-mirror-license-refused) |
