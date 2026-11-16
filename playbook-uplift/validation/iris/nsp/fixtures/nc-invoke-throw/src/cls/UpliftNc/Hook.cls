Class UpliftNc.Hook
{

ClassMethod Boom()
{
    set ^IRIS.Temp.NcHookRan = $get(^IRIS.Temp.NcHookRan) + 1
    throw ##class(%Exception.General).%New("NcHookThrow", 5001, , "nc-invoke-throw hook threw")
}

}
