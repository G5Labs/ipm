Class UpDepHalt.Hooks
{

ClassMethod Halt() As %Status
{
    set ^IRIS.Temp.UpliftSignal("dephalt") = $listbuild($job, $tlevel)
    halt
}

}
