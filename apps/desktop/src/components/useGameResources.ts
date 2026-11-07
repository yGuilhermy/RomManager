import { useEffect, useRef, useState } from "react";
import { apiJson, cachedPcsx2Json, invalidatePcsx2Cache } from "../lib/api";
import { resourceQueue } from "../lib/resourceQueue";
import type { NativeState, ResourceContext, ResourceKind, ResourcePlan } from "../lib/gameResourceTypes";

const snapshots = new Map<string, NativeState>();
type Pending = Record<string,"queued"|"saving">;
export function useGameResources(gameId:string, serial:string|null, kind:ResourceKind) {
  const [context,setContext]=useState<ResourceContext|null>(null);
  const [native,setNative]=useState<NativeState|null>(null);
  const [crc,setCrc]=useState("");
  const [reading,setReading]=useState(false);
  const [pending,setPending]=useState<Pending>({});
  const [errors,setErrors]=useState<Record<string,string>>({});
  const [notice,setNotice]=useState("");
  const mounted=useRef(true),contextRef=useRef<ResourceContext|null>(null);
  const owner=useRef({});
  const nativeRequired=kind!=="covers"&&kind!=="saves";
  const remember=(data:NativeState)=>{
    snapshots.set(`${gameId}|${data.crc}`,data);
    if(snapshots.size>32)snapshots.delete(snapshots.keys().next().value!);
    if(mounted.current){setNative(data);if(!crc&&data.crc)setCrc(data.crc);}
  };
  const clearError=(key:string)=>setErrors(old=>{const next={...old};delete next[key];return next;});
  const read=async(force=false)=>{
    if(force)invalidatePcsx2Cache();
    const data=await cachedPcsx2Json<ResourceContext>("/api/pcsx2/resources");
    contextRef.current=data;if(mounted.current)setContext(data);
    if(serial)remember(await cachedPcsx2Json<NativeState>(`/api/pcsx2/resources/game?game_id=${encodeURIComponent(gameId)}&crc=${encodeURIComponent(crc)}`));
    return data;
  };
  const refresh=async(force=true)=>{
    setReading(true);clearError("read");
    try{await resourceQueue.run(()=>read(force));}
    catch(e){if(mounted.current)setErrors(old=>({...old,read:e instanceof Error?e.message:"Não foi possível ler os recursos"}));}
    finally{if(mounted.current)setReading(false);}
  };
  useEffect(()=>{
    mounted.current=true;
    if(nativeRequired)void refresh(false);
    const changed=(event:Event)=>{
      const detail=(event as CustomEvent).detail;
      if(detail?.gameId===gameId&&detail?.owner!==owner.current&&nativeRequired)void refresh(true);
    };
    window.addEventListener("rommanager:resources-changed",changed);
    return()=>{mounted.current=false;window.removeEventListener("rommanager:resources-changed",changed);};
  },[gameId]);
  const run=async<T,>(key:string,operation:()=>Promise<T>):Promise<T|undefined>=>{
    if(pending[key])return;
    setPending(old=>({...old,[key]:"queued"}));clearError(key);setNotice("");
    try{return await resourceQueue.run(async()=>{
      if(mounted.current)setPending(old=>({...old,[key]:"saving"}));
      let result:T;
      try {result=await operation();} catch(error) {try{await read(true);}catch{}throw error;}
      window.dispatchEvent(new CustomEvent("rommanager:resources-changed",{detail:{gameId,owner:owner.current}}));
      return result;
    });}catch(e){
      if(mounted.current)setErrors(old=>({...old,[key]:e instanceof Error?e.message:"Não foi possível concluir a operação"}));
      return undefined;
    }finally{if(mounted.current)setPending(old=>{const next={...old};delete next[key];return next;});}
  };
  const toggle=async(enabled:boolean,groupName?:string)=>{
    const key=groupName===undefined?"master":`group:${groupName}`;
    await run(key,async()=>{
      const latest=snapshots.get(`${gameId}|${crc}`)||native;
      const data=await apiJson<NativeState>("/api/pcsx2/resources/game",{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({game_id:gameId,crc,kind,enabled,group_name:groupName,revision:latest?.revision,installation_id:contextRef.current?.installation_id})});
      remember(data);
      if(mounted.current)setNotice(`${groupName||({cheats:"Execução de cheats",patches:"Correções automáticas",textures:"Texturas"} as Record<string,string>)[kind]}: ${enabled?"ativado":"desativado"}`);
    });
  };
  const install=async(action:string,source:string,planId?:string)=>run(action==="undo"?`undo:${planId}`:"import",async()=>{
    const data=await apiJson<ResourcePlan>("/api/pcsx2/resources",{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({action,game_id:gameId,kind,source,crc,plan_id:planId,installation_id:contextRef.current?.installation_id})});
    if(action!=="preview"){
      await read(true);
      if(mounted.current)setNotice(action==="undo"?"Recurso removido; arquivos anteriores restaurados":"Recurso instalado");
    }
    return data;
  });
  return {report:setNotice,context,native:native?.crc===crc?native:null,revisionOptions:native?.available_crcs||[],crc,setCrc,reading,pending,errors,notice,run,refresh,toggle,install};
}
