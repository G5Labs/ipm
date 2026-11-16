Class NcA2O17F.Upd.V2 Extends %IPM.General.Update.VersionBase
{

ClassMethod GetOrderedMethods() As %Library.DynamicArray
{
    return ["A", "B"]
}

ClassMethod A()
{
    set ^NcA2O17F("V2.A") = $get(^NcA2O17F("V2.A")) + 1
}

/// Fails on purpose.
ClassMethod B()
{
    throw ##class(%Exception.General).%New("NcA2O17F V2.B fails on purpose")
}

}
