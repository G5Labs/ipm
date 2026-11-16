Class Consolidated.Iter3.FailDocument Extends %IPM.ResourceProcessor.Default.Document
{
Method OnExportDeployedItem(path, item, ByRef itemParams, ByRef params, Output handled As %Boolean = 0) As %Status
{
    set handled=0
    quit $$$ERROR($$$GeneralError,"controlled deployed-item failure")
}
}
