/// Probe record shaped like %IPM.General.History: a Finalized flag guarded by an UPDATE trigger.
Class Uplift.Probe.Rec Extends %Persistent
{

Property Name As %String;

Property Committed As %Boolean [ InitialExpression = 0 ];

Property Finalized As %Boolean [ InitialExpression = 0 ];

Trigger PreventSQLUpdates [ Event = UPDATE, Foreach = row/object ]
{
    if {Finalized*O} = 1 {
        set %msg = "Cannot edit finalized probe records"
        set %ok = 0
    }
}

}
