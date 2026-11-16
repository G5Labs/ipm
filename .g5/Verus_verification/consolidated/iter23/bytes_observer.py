"""Observe original byte streams; escaped session text is never the byte oracle."""
import re
ANSI=rb'\x1b\[[0-?]*[ -/]*[@-~]'
def plain(value):return re.sub(ANSI,b'',value)
def utf8(value):
 try:value.decode('utf-8');return 1
 except UnicodeDecodeError:return 0
def fmt(code,value,color):return b'\x1b['+str(code).encode()+b'm'+value+b'\x1b[0m' if color else value
def cleanup(value,color=False):
 return value.replace(b'\r',fmt(2,b'<CR>',color)).replace(b'\n',fmt(2,b'<LF>',color)).replace(b'\t',fmt(2,b'<TAB>',color))
def segment(raw,label):
 start=b'BEGIN|'+label.encode()+b'\n';end=b'END|'+label.encode()+b'\n'
 assert raw.count(start)==1,(label,'missing/duplicate start')
 part=raw.split(start,1)[1];assert end in part,label
 return part.split(end,1)[0]
def observe(raw):
 raw=raw.replace(b'\r\n',b'\n');out={}
 for label in ['good','controls','bad']:
  fields={}
  for kind in ['raw','decoded']:
   matches=re.findall(rb'^HEX\|'+label.encode()+b'\\|'+kind.encode()+rb'\|([0-9A-Fa-f]*)$',raw,re.M)
   assert len(matches)==1,(label,kind,len(matches));fields[kind]=bytes.fromhex(matches[0].decode())
  ids=re.findall(rb'^ROWID\|'+label.encode()+rb'\|(\d+)$',raw,re.M);assert len(ids)==1,(label,ids)
  out[label+'-stored-raw-utf8']=utf8(fields['raw']);out[label+'-decoded-utf8']=utf8(fields['decoded'])
  for control,value in [('cr',b'\r'),('lf',b'\n'),('tab',b'\t')]:out[label+'-raw-has-'+control]=int(value in fields['raw'])
  for suffix,color in [('',True),('-none',False)]:
   key=label+suffix;summary=segment(raw,'summary-'+key);detail=segment(raw,'detail-'+key)
   lines=re.findall(rb'^Success: ([^\n]*)$',plain(detail),re.M);assert len(lines)==1,(key,lines);value=lines[0]
   exact_lines=[line for line in detail.split(b'\n') if plain(line).startswith(b'Success: ')];assert len(exact_lines)==1,key
   expected=fmt(2,b'Success',color)+b': '+fmt(39,cleanup(fields['raw'],color),color)
   rows=re.findall(rb'^\s*'+ids[0]+rb'\s+install\s+[^\n]*$',plain(summary),re.M);assert len(rows)==1,(key,rows)
   wanted=b'Success' if label=='good' else cleanup(fields['decoded'])
   out.update({key+'-summary-utf8':utf8(summary),key+'-detail-utf8':utf8(detail),key+'-detail-status-utf8':utf8(value),key+'-detail-equals-cleaned-raw':int(value==cleanup(fields['raw'])),key+'-detail-exact-format':int(exact_lines[0]==expected),key+'-summary-has-status':int(wanted in rows[0]),key+'-detail-status-length':len(value),key+'-emitted-status-line-length':len(exact_lines[0]),key+'-summary-row-found':1,key+'-has-ansi':int(b'\x1b[' in detail)})
 return out
