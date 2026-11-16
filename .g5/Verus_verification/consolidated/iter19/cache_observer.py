"""Extract constructor per-entry output and separately sampled physical scan data."""
import re

def observe(raw):
    raw=re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]','',raw).replace('\r\n','\n')
    result={}
    for label in ['single','ab','ba']:
        begin='BEGIN|repo-'+label+'\n';end='END|repo-'+label+'\n'
        assert raw.count(begin)==raw.count(end)==1,label
        part=raw.split(begin,1)[1].split(end,1)[0]
        dirs=re.findall(r'^iter19-dupe 1\.0\.0 @ /tmp/iter19/repos/'+label+r'/([^/]+)/$',part,re.M)
        sampled=[];markers={}
        for line in raw.splitlines():
            if line.startswith('SCAN|'+label+'|'):
                row=line.split('|');assert len(row)==6,row
                sampled.append((int(row[2]),row[3]));markers[row[3]]=int(row[5])
        sampled=[sub for _,sub in sorted(sampled)]
        assert dirs and len(set(dirs))==len(dirs) and set(dirs)<=set(markers),label
        result[label+'-constructor-count']=len(dirs)
        result[label+'-constructor-agrees-sampled-scan']=int(dirs==sampled)
        for i,sub in enumerate(dirs,1):result[label+'-constructor-'+str(i)+'-marker']=markers[sub]
    return result
