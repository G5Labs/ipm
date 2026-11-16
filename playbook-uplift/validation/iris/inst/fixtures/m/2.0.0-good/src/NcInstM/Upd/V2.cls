Class NcInstM.Upd.V2 Extends %IPM.General.Update.VersionBase
{

ClassMethod GetOrderedMethods() As %Library.DynamicArray
{
    return ["A", "B"]
}

/// Ordinary global (rolled back), $INCREMENT counter (not rolled back) and a file line (not rolled back).
ClassMethod A()
{
    set ^NcInstMx = $get(^NcInstMx) + 1
    if $increment(^NcInstMy)
    set io = $io, f = "/tmp/nc-inst-m-steps.txt"
    open f:("WAS"):2
    if $test { use f write "A ran", ! close f }
    use io
}

ClassMethod B()
{
    set ^NcInstM("V2.B") = $get(^NcInstM("V2.B")) + 1
}

}
