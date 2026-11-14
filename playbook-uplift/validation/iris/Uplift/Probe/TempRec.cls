/// Probe record shaped like %IPM.General.HistoryTemp: stored in ^IRIS.Temp.*, holds a twin Rec,
/// and copies itself to the twin in %OnClose.
Class Uplift.Probe.TempRec Extends %Persistent
{

Property Name As %String;

Property Committed As %Boolean [ InitialExpression = 0 ];

Property Twin As Uplift.Probe.Rec;

ClassMethod Init(name As %String) As Uplift.Probe.TempRec
{
    set log = ..%New()
    set log.Name = name
    $$$ThrowOnError(log.%Save())
    set twin = ##class(Uplift.Probe.Rec).%New()
    set twin.Name = name
    $$$ThrowOnError(twin.%Save())
    set log.Twin = twin
    $$$ThrowOnError(log.%Save())
    quit log
}

Method PersistToTwin() As %Status
{
    $$$ThrowOnError(..%Reload())
    set ..Twin.Name = ..Name
    set ..Twin.Committed = ..Committed
    set ..Twin.Finalized = 1
    quit ..%Save()
}

Method %OnClose() As %Status [ Private, ServerOnly = 1 ]
{
    set ^UpProbe("onclose", $increment(^UpProbe("onclose"))) = $listbuild(..Name, $tlevel, $job)
    set sc = ..PersistToTwin()
    set ^UpProbe("onclose", ^UpProbe("onclose"), "sc") = $select($system.Status.IsOK(sc): "OK", 1: $system.Status.GetErrorText(sc))
    quit $$$ADDSC(sc, ..%DeleteId(..%Id()))
}

Storage Default
{
<Data name="TempRecDefaultData">
<Value name="1">
<Value>%%CLASSNAME</Value>
</Value>
<Value name="2">
<Value>Name</Value>
</Value>
<Value name="3">
<Value>Committed</Value>
</Value>
<Value name="4">
<Value>Twin</Value>
</Value>
</Data>
<DataLocation>^IRIS.Temp.UpProbeTempD</DataLocation>
<DefaultData>TempRecDefaultData</DefaultData>
<IdLocation>^IRIS.Temp.UpProbeTempD</IdLocation>
<IndexLocation>^IRIS.Temp.UpProbeTempI</IndexLocation>
<StreamLocation>^IRIS.Temp.UpProbeTempS</StreamLocation>
<Type>%Storage.Persistent</Type>
}

}
