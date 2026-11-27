
function utf8Bytes(value){return Uint8Array.from(unescape(encodeURIComponent(value)),c=>c.charCodeAt(0));}
function concatBytes(parts){const out=new Uint8Array(parts.reduce((n,p)=>n+p.length,0));let offset=0;for(const p of parts){out.set(p,offset);offset+=p.length;}return out;}
function padHash(input){const size=Math.ceil((input.length+9)/64)*64,out=new Uint8Array(size);out.set(input);out[input.length]=128;const view=new DataView(out.buffer);view.setUint32(size-8,Math.floor(input.length/536870912),false);view.setUint32(size-4,(input.length*8)>>>0,false);return out;}
function sha256Bytes(input){
 const K=[0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2];
 const h=new Uint32Array([0x6a09e667,0xbb67ae85,0x3c6ef372,0xa54ff53a,0x510e527f,0x9b05688c,0x1f83d9ab,0x5be0cd19]);
 const bytes=padHash(input),view=new DataView(bytes.buffer),w=new Uint32Array(64);
 const r=(x,n)=>(x>>>n)|(x<<(32-n));
 for(let off=0;off<bytes.length;off+=64){
  for(let i=0;i<16;i++)w[i]=view.getUint32(off+4*i,false);
  for(let i=16;i<64;i++){const x=w[i-15],y=w[i-2];w[i]=(w[i-16]+(r(x,7)^r(x,18)^(x>>>3))+w[i-7]+(r(y,17)^r(y,19)^(y>>>10)))>>>0;}
  let[a,b,c,d,e,f,g,z]=h;
  for(let i=0;i<64;i++){const t1=(z+(r(e,6)^r(e,11)^r(e,25))+((e&f)^(~e&g))+K[i]+w[i])>>>0,t2=((r(a,2)^r(a,13)^r(a,22))+((a&b)^(a&c)^(b&c)))>>>0;z=g;g=f;f=e;e=(d+t1)>>>0;d=c;c=b;b=a;a=(t1+t2)>>>0;}
  const s=[a,b,c,d,e,f,g,z];for(let i=0;i<8;i++)h[i]=(h[i]+s[i])>>>0;
 }
 return Array.from(h,x=>x.toString(16).padStart(8,"0")).join("");
}
function sha256(value){return sha256Bytes(utf8Bytes(value));}
function sha1Bytes(input){
 const h=new Uint32Array([0x67452301,0xefcdab89,0x98badcfe,0x10325476,0xc3d2e1f0]),bytes=padHash(input),view=new DataView(bytes.buffer),w=new Uint32Array(80);
 const l=(x,n)=>(x<<n)|(x>>>(32-n));
 for(let off=0;off<bytes.length;off+=64){
  for(let i=0;i<16;i++)w[i]=view.getUint32(off+4*i,false);
  for(let i=16;i<80;i++)w[i]=l(w[i-3]^w[i-8]^w[i-14]^w[i-16],1);
  let[a,b,c,d,e]=h;
  for(let i=0;i<80;i++){let f,k;if(i<20){f=(b&c)|(~b&d);k=0x5a827999;}else if(i<40){f=b^c^d;k=0x6ed9eba1;}else if(i<60){f=(b&c)|(b&d)|(c&d);k=0x8f1bbcdc;}else{f=b^c^d;k=0xca62c1d6;}const t=(l(a,5)+f+e+k+w[i])>>>0;e=d;d=c;c=l(b,30);b=a;a=t;}
  const s=[a,b,c,d,e];for(let i=0;i<5;i++)h[i]=(h[i]+s[i])>>>0;
 }
 return Array.from(h,x=>x.toString(16).padStart(8,"0")).join("");
}
function gitObject(kind,bytes){return sha1Bytes(concatBytes([utf8Bytes(kind+" "+bytes.length+"\0"),bytes]));}
function gitBlob(content){return gitObject("blob",utf8Bytes(content));}
function hexBytes(value){if(!/^[0-9a-f]{40}$/.test(value))throw Error("invalid object SHA");return Uint8Array.from(value.match(/../g),x=>parseInt(x,16));}
function gitTree(rows){
 const root={dirs:new Map(),files:new Map()};
 for(const row of rows){const bits=row.path.split("/");if(bits.some(x=>!x||x==="."||x===".."))throw Error("invalid path");let dir=root;for(const name of bits.slice(0,-1)){if(dir.files.has(name))throw Error("file/directory overlap");if(!dir.dirs.has(name))dir.dirs.set(name,{dirs:new Map(),files:new Map()});dir=dir.dirs.get(name);}const name=bits.at(-1);if(dir.files.has(name)||dir.dirs.has(name))throw Error("duplicate path");dir.files.set(name,row);}
 function walk(dir){const entries=[];for(const[name,row]of dir.files)entries.push({name,mode:row.mode,sha:row.sha,key:utf8Bytes(name)});for(const[name,child]of dir.dirs)entries.push({name,mode:"40000",sha:walk(child),key:utf8Bytes(name+"/")});entries.sort((a,b)=>{for(let i=0;i<Math.min(a.key.length,b.key.length);i++)if(a.key[i]!==b.key[i])return a.key[i]-b.key[i];return a.key.length-b.key.length;});return gitObject("tree",concatBytes(entries.map(e=>concatBytes([utf8Bytes(e.mode+" "+e.name+"\0"),hexBytes(e.sha)]))));}
 return walk(root);
}
function decodeBase64(value){
 if(!/^[A-Za-z0-9+/]*={0,2}$/.test(value)||value.length%4!==0)throw Error("invalid base64");const alphabet="ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/",padding=value.endsWith("==")?2:value.endsWith("=")?1:0,out=new Uint8Array(value.length/4*3-padding);let bits=0,word=0,offset=0;for(const char of value){if(char==="=")break;word=(word<<6)|alphabet.indexOf(char);bits+=6;if(bits>=8){bits-=8;out[offset++]=(word>>>bits)&255;}}if(offset!==out.length)throw Error("base64 size");return out;
}
