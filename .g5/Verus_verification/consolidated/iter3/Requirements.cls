Class Consolidated.Iter3.Requirements Extends %IPM.Storage.SystemRequirements
{
ClassMethod IsHealthInstance() As %Boolean
{
    quit $get(^||I3Health)
}
ClassMethod IsInteroperabilityEnabled() As %Boolean
{
    quit $get(^||I3Interop)
}
}
