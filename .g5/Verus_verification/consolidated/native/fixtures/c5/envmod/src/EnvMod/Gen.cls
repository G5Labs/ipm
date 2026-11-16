Class EnvMod.Gen
{

ClassMethod Val() As %String [ CodeMode = objectgenerator ]
{
    set v = ##class(%IPM.General.EnvironmentConfig).%Get().GetArg("envmod", "greeting")
    do %code.WriteLine(" quit """_$replace(v,"""","""""")_"""")
    quit $$$OK
}

}
