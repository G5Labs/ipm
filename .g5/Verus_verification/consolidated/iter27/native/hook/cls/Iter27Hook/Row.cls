Class Iter27Hook.Row Extends %Persistent [ SqlTableName = RowRecord ]
{
Property Token As %String(MAXLEN=30) [ Required ];
Property Val As %Integer;
Index TokenKey On Token [ Unique ];
ClassMethod Marker() { quit 27 }
}
