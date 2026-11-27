function reconcileSpiraLog(raw, marker, hashes) {
 const lines=raw.split("\n").map(line=>line.replace(/^\uFEFF?\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d+Z ?/,""));
 const terminal=lines.map((line,index)=>({line,index})).filter(x=>x.line.startsWith(marker));
 if(terminal.length!==1)throw Error("terminal report count "+terminal.length);
 const reportText=terminal[0].line.slice(marker.length);
 const report=JSON.parse(reportText);
 const starts=[],results=[];
 for(let i=0;i<lines.length;i++){
   if(lines[i].startsWith("SPIRA_COMMAND_START "))starts.push({index:i,row:JSON.parse(lines[i].slice(20))});
   if(lines[i].startsWith("SPIRA_COMMAND_RESULT "))results.push({index:i,row:JSON.parse(lines[i].slice(21))});
 }
 if(starts.length!==results.length||results.length!==report.commands.length)throw Error("command receipt cardinality");
 const verified=[],outputs={};
 for(let i=0;i<results.length;i++){
   const start=starts[i], result=results[i], row=report.commands[i];
   if(start.index>=result.index||result.index>=(starts[i+1]?.index??terminal[0].index))throw Error("command order "+i);
   if(JSON.stringify(result.row)!==JSON.stringify(row)||start.row.name!==row.name||JSON.stringify(start.row.command)!==JSON.stringify(row.command))throw Error("receipt identity "+i);
   const limit=starts[i+1]?.index??terminal[0].index;
   const ends=new Set([result.index+1,limit]);
   for(let end=result.index+1;end<limit;end++)if(/^[A-Z][A-Z0-9_]+ \{/.test(lines[end]))ends.add(end);
   const matches=new Map();
   for(const end of ends){
     const full=lines.slice(result.index+1,end).join("\n")+(end>result.index+1?"\n":"");
     for(const data of [full,full.endsWith("\n")?full.slice(0,-1):full]){
       if(hashes.sha256(data)===row.log_sha256)matches.set(data,data);
     }
   }
   if(matches.size!==1)throw Error("stdout reconstruction "+row.name+" matches="+matches.size+" region="+(limit-result.index));
   const data=[...matches.values()][0];outputs[row.name]=data;
   verified.push({name:row.name,sha256:row.log_sha256,bytes:hashes.utf8Bytes(data).length});
 }
 return {report,inlineReport:reportText,reportText:JSON.stringify(report,null,2)+"\n",rawSha256:hashes.sha256(raw),verified,outputs,unexpected:report.commands.filter(row=>row.raw_exit!==row.expected||row.timed_out)};
}
