# Pinned source decisions and consuming readers

These are scoped source bindings, not direct implementation proofs.

| Case | Role | Exact pinned member |
|---|---|---|
| S24-01 | direct | [src/cls/IPM/General/SemanticVersion.cls::FromString](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/SemanticVersion.cls#L33) |
| S24-01 | direct | [src/cls/IPM/General/SemanticVersion.cls::IsSnapshot](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/SemanticVersion.cls#L98) |
| S24-01 | direct | [src/cls/IPM/General/SemanticVersion.cls::Satisfies](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/SemanticVersion.cls#L207) |
| S24-01 | direct | [src/cls/IPM/General/SemanticVersionExpression.cls::FromString](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/SemanticVersionExpression.cls#L93) |
| S24-01 | direct | [src/cls/IPM/General/SemanticVersionExpression.cls::IsSatisfiedBy](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/SemanticVersionExpression.cls#L106) |
| S24-01 | direct | [src/cls/IPM/General/SemanticVersionExpression.cls::And](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/SemanticVersionExpression.cls#L129) |
| S24-01 | direct | [src/cls/IPM/General/SemanticVersionExpression/Comparator.cls::Evaluate](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/SemanticVersionExpression/Comparator.cls#L77) |
| S24-01 | direct | [src/cls/IPM/General/SemanticVersionExpression/Range.cls::Intersect](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/SemanticVersionExpression/Range.cls#L228) |
| S24-01 | direct | [src/cls/IPM/General/SemanticVersionExpression/Comparator.cls::ExtractToSemanticVersion](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/SemanticVersionExpression/Comparator.cls#L131) |
| S24-01 | reader | [src/cls/IPM/Storage/Module.cls::ProcessSingleDependencyIterative](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/Module.cls#L1069) |
| S24-02 | direct | [src/cls/IPM/Main.cls::Reinstall](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L2673) |
| S24-02 | direct | [src/cls/IPM/Main.cls::Install](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L2554) |
| S24-02 | direct | [src/cls/IPM/Utils/Module.cls::LoadDependencies](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L1368) |
| S24-02 | direct | [src/cls/IPM/Storage/Module.cls::BuildDependencyGraph](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/Module.cls#L898) |
| S24-02 | direct | [src/cls/IPM/Storage/Module.cls::ProcessSingleDependencyIterative](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/Module.cls#L1069) |
| S24-02 | reader | [src/cls/IPM/Utils/Module.cls::ConstructInvertedDependencyGraph](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L1428) |
| S24-02 | reader | [src/cls/IPM/Utils/Module.cls::GetFlatDependencyListFromInvertedDependencyGraph](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L1457) |
| S24-02 | reader | [src/cls/IPM/Utils/Module.cls::LoadDependencies](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L1368) |
| S24-03 | direct | [src/cls/IPM/Utils/Module.cls::LoadDependencies](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L1368) |
| S24-03 | direct | [src/cls/IPM/Utils/Module.cls::LoadModuleReference](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L40) |
| S24-03 | direct | [src/cls/IPM/Repo/Filesystem/PackageService.cls::GetModuleDirectory](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Filesystem/PackageService.cls#L48) |
| S24-03 | direct | [src/cls/IPM/Utils/Module.cls::LoadModuleFromDirectory](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L293) |
| S24-03 | direct | [src/cls/IPM/Utils/Module.cls::LoadNewModule](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L1152) |
| S24-03 | reader | [src/cls/IPM/Storage/Module.cls::ExecutePhases](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/Module.cls#L447) |
