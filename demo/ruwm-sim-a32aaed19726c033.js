let U=128,a3=`boolean`,a7=4,a0=1,X=`undefined`,a1=`function`,a9=33,W=null,a4=`string`,a2=`number`,a5=`Object`,_=0,Y=`utf-8`,T=Array,Z=Error,a6=FinalizationRegistry,a8=Object,$=Uint8Array,V=undefined;var e=(a=>{if(a<132)return;b[a]=d;d=a});var K=(()=>{if(J===W||J.byteLength===_){J=new Uint8ClampedArray(a.memory.buffer)};return J});var S=(async(b)=>{if(a!==V)return a;if(typeof b===X){b=new URL(`ruwm-sim_bg.wasm`,import.meta.url)};const c=O();if(typeof b===a4||typeof Request===a1&&b instanceof Request||typeof URL===a1&&b instanceof URL){b=fetch(b)};P(c);const {instance:d,module:e}=await N(await b,c);return Q(d,e)});var j=((a,b)=>{a=a>>>_;return g.decode(i().subarray(a,a+ b))});var A=((c,d,e)=>{try{a.wasm_bindgen__convert__closures__invoke1_mut_ref__h52837986ee145cb1(c,d,z(e))}finally{b[y++]=V}});var C=((b,c,d,e)=>{const f={a:b,b:c,cnt:a0,dtor:d};const g=(...b)=>{f.cnt++;try{return e(f.a,f.b,...b)}finally{if(--f.cnt===_){a.__wbindgen_export_2.get(f.dtor)(f.a,f.b);f.a=_;v.unregister(f)}}};g.original=f;v.register(g,f,f);return g});var P=((a,b)=>{});var w=((b,c,d,e)=>{const f={a:b,b:c,cnt:a0,dtor:d};const g=(...b)=>{f.cnt++;const c=f.a;f.a=_;try{return e(c,f.b,...b)}finally{if(--f.cnt===_){a.__wbindgen_export_2.get(f.dtor)(c,f.b);v.unregister(f)}else{f.a=c}}};g.original=f;v.register(g,f,f);return g});var E=(a=>()=>{throw new Z(`${a} is not defined`)});var D=((c,d,e)=>{try{a.wasm_bindgen__convert__closures__invoke1_ref__hdcc0400638fe7ff0(c,d,z(e))}finally{b[y++]=V}});var f=(a=>{const b=c(a);e(a);return b});var I=((a,b)=>{a=a>>>_;const c=H();const d=c.subarray(a/a7,a/a7+ b);const e=[];for(let a=_;a<d.length;a++){e.push(f(d[a]))};return e});var r=(()=>{if(q===W||q.byteLength===_){q=new Int32Array(a.memory.buffer)};return q});var x=((b,c)=>{a.wasm_bindgen__convert__closures__invoke0_mut__ha499f5fd9ab18d08(b,c)});var z=(a=>{if(y==a0)throw new Z(`out of js stack`);b[--y]=a;return y});var i=(()=>{if(h===W||h.byteLength===_){h=new $(a.memory.buffer)};return h});var o=((a,b,c)=>{if(c===V){const c=m.encode(a);const d=b(c.length,a0)>>>_;i().subarray(d,d+ c.length).set(c);l=c.length;return d};let d=a.length;let e=b(d,a0)>>>_;const f=i();let g=_;for(;g<d;g++){const b=a.charCodeAt(g);if(b>127)break;f[e+ g]=b};if(g!==d){if(g!==_){a=a.slice(g)};e=c(e,d,d=g+ a.length*3,a0)>>>_;const b=i().subarray(e+ g,e+ d);const f=n(a,b);g+=f.written;e=c(e,d,g,a0)>>>_};l=g;return e});var R=(b=>{if(a!==V)return a;const c=O();P(c);if(!(b instanceof WebAssembly.Module)){b=new WebAssembly.Module(b)};const d=new WebAssembly.Instance(b,c);return Q(d,b)});var M=((a,b)=>{a=a>>>_;return i().subarray(a/a0,a/a0+ b)});var H=(()=>{if(G===W||G.byteLength===_){G=new Uint32Array(a.memory.buffer)};return G});var N=(async(a,b)=>{if(typeof Response===a1&&a instanceof Response){if(typeof WebAssembly.instantiateStreaming===a1){try{return await WebAssembly.instantiateStreaming(a,b)}catch(b){if(a.headers.get(`Content-Type`)!=`application/wasm`){console.warn(`\`WebAssembly.instantiateStreaming\` failed because your server does not serve wasm with \`application/wasm\` MIME type. Falling back to \`WebAssembly.instantiate\` which is slower. Original error:\\n`,b)}else{throw b}}};const c=await a.arrayBuffer();return await WebAssembly.instantiate(c,b)}else{const c=await WebAssembly.instantiate(a,b);if(c instanceof WebAssembly.Instance){return {instance:c,module:a}}else{return c}}});var k=(a=>{if(d===b.length)b.push(b.length+ a0);const c=d;d=b[c];b[c]=a;return c});var c=(a=>b[a]);var t=(()=>{if(s===W||s.byteLength===_){s=new Float64Array(a.memory.buffer)};return s});var L=((a,b)=>{a=a>>>_;return K().subarray(a/a0,a/a0+ b)});var Q=((b,c)=>{a=b.exports;S.__wbindgen_wasm_module=c;s=W;q=W;G=W;h=W;J=W;a.__wbindgen_start();return a});function F(b,c){try{return b.apply(this,c)}catch(b){a.__wbindgen_exn_store(k(b))}}var p=(a=>a===V||a===W);var B=((b,c,d)=>{a.wasm_bindgen__convert__closures__invoke1_mut__h00fcd13fedbf1919(b,c,k(d))});var u=(a=>{const b=typeof a;if(b==a2||b==a3||a==W){return `${a}`};if(b==a4){return `"${a}"`};if(b==`symbol`){const b=a.description;if(b==W){return `Symbol`}else{return `Symbol(${b})`}};if(b==a1){const b=a.name;if(typeof b==a4&&b.length>_){return `Function(${b})`}else{return `Function`}};if(T.isArray(a)){const b=a.length;let c=`[`;if(b>_){c+=u(a[_])};for(let d=a0;d<b;d++){c+=`, `+ u(a[d])};c+=`]`;return c};const c=/\[object ([^\]]+)\]/.exec(toString.call(a));let d;if(c.length>a0){d=c[a0]}else{return toString.call(a)};if(d==a5){try{return `Object(`+ JSON.stringify(a)+ `)`}catch(a){return a5}};if(a instanceof Z){return `${a.name}: ${a.message}\n${a.stack}`};return d});var O=(()=>{const b={};b.wbg={};b.wbg.__wbg_new_abda76e883ba8a5f=(()=>{const a=new Z();return k(a)});b.wbg.__wbg_stack_658279fe44541cf6=((b,d)=>{const e=c(d).stack;const f=o(e,a.__wbindgen_malloc,a.__wbindgen_realloc);const g=l;r()[b/a7+ a0]=g;r()[b/a7+ _]=f});b.wbg.__wbg_error_f851667af71bcfc6=((b,c)=>{let d;let e;try{d=b;e=c;console.error(j(b,c))}finally{a.__wbindgen_free(d,e,a0)}});b.wbg.__wbindgen_object_drop_ref=(a=>{f(a)});b.wbg.__wbindgen_cb_drop=(a=>{const b=f(a).original;if(b.cnt--==a0){b.a=_;return !0};const c=!1;return c});b.wbg.__wbg_clearTimeout_efaca71ee1e15036=typeof clearTimeout==a1?clearTimeout:E(`clearTimeout`);b.wbg.__wbg_setTimeout_aeb3246e9bf3f9d3=((a,b)=>{const d=setTimeout(c(a),b>>>_);return d});b.wbg.__wbindgen_is_object=(a=>{const b=c(a);const d=typeof b===`object`&&b!==W;return d});b.wbg.__wbg_crypto_566d7465cdbb6b7a=(a=>{const b=c(a).crypto;return k(b)});b.wbg.__wbg_process_dc09a8c7d59982f6=(a=>{const b=c(a).process;return k(b)});b.wbg.__wbg_versions_d98c6400c6ca2bd8=(a=>{const b=c(a).versions;return k(b)});b.wbg.__wbg_node_caaf83d002149bd5=(a=>{const b=c(a).node;return k(b)});b.wbg.__wbindgen_is_string=(a=>{const b=typeof c(a)===a4;return b});b.wbg.__wbg_require_94a9da52636aacbf=function(){return F((()=>{const a=module.require;return k(a)}),arguments)};b.wbg.__wbindgen_is_function=(a=>{const b=typeof c(a)===a1;return b});b.wbg.__wbindgen_string_new=((a,b)=>{const c=j(a,b);return k(c)});b.wbg.__wbg_call_b3ca7c6051f9bec1=function(){return F(((a,b,d)=>{const e=c(a).call(c(b),c(d));return k(e)}),arguments)};b.wbg.__wbg_msCrypto_0b84745e9245cdf6=(a=>{const b=c(a).msCrypto;return k(b)});b.wbg.__wbg_newwithlength_e9b4878cebadb3d3=(a=>{const b=new $(a>>>_);return k(b)});b.wbg.__wbg_removeEventListener_5d31483804421bfa=function(){return F(((a,b,d,e,f)=>{c(a).removeEventListener(j(b,d),c(e),f!==_)}),arguments)};b.wbg.__wbindgen_object_clone_ref=(a=>{const b=c(a);return k(b)});b.wbg.__wbg_new_72fb9a18b5ae2624=(()=>{const a=new a8();return k(a)});b.wbg.__wbg_location_2951b5ee34f19221=(a=>{const b=c(a).location;return k(b)});b.wbg.__wbg_newwithbase_6aabbfb1b2e6a1cb=function(){return F(((a,b,c,d)=>{const e=new URL(j(a,b),j(c,d));return k(e)}),arguments)};b.wbg.__wbg_search_c68f506c44be6d1e=((b,d)=>{const e=c(d).search;const f=o(e,a.__wbindgen_malloc,a.__wbindgen_realloc);const g=l;r()[b/a7+ a0]=g;r()[b/a7+ _]=f});b.wbg.__wbg_hash_cdea7a9b7e684a42=((b,d)=>{const e=c(d).hash;const f=o(e,a.__wbindgen_malloc,a.__wbindgen_realloc);const g=l;r()[b/a7+ a0]=g;r()[b/a7+ _]=f});b.wbg.__wbindgen_memory=(()=>{const b=a.memory;return k(b)});b.wbg.__wbg_buffer_12d079cc21e14bdb=(a=>{const b=c(a).buffer;return k(b)});b.wbg.__wbg_newwithbyteoffsetandlength_aa4a17c33a06e5cb=((a,b,d)=>{const e=new $(c(a),b>>>_,d>>>_);return k(e)});b.wbg.__wbg_randomFillSync_290977693942bf03=function(){return F(((a,b)=>{c(a).randomFillSync(f(b))}),arguments)};b.wbg.__wbg_subarray_a1f73cd4b5b42fe1=((a,b,d)=>{const e=c(a).subarray(b>>>_,d>>>_);return k(e)});b.wbg.__wbg_getRandomValues_260cc23a41afad9a=function(){return F(((a,b)=>{c(a).getRandomValues(c(b))}),arguments)};b.wbg.__wbindgen_number_new=(a=>{const b=a;return k(b)});b.wbg.__wbg_set_f975102236d3c502=((a,b,d)=>{c(a)[f(b)]=f(d)});b.wbg.__wbg_state_9cc3f933b7d50acb=function(){return F((a=>{const b=c(a).state;return k(b)}),arguments)};b.wbg.__wbg_getwithrefkey_edc2c8960f0f1191=((a,b)=>{const d=c(a)[c(b)];return k(d)});b.wbg.__wbindgen_is_undefined=(a=>{const b=c(a)===V;return b});b.wbg.__wbindgen_in=((a,b)=>{const d=c(a) in c(b);return d});b.wbg.__wbg_entries_95cc2c823b285a09=(a=>{const b=a8.entries(c(a));return k(b)});b.wbg.__wbg_length_cd7af8117672b8b8=(a=>{const b=c(a).length;return b});b.wbg.__wbg_get_bd8e338fbd5f5cc8=((a,b)=>{const d=c(a)[b>>>_];return k(d)});b.wbg.__wbindgen_string_get=((b,d)=>{const e=c(d);const f=typeof e===a4?e:V;var g=p(f)?_:o(f,a.__wbindgen_malloc,a.__wbindgen_realloc);var h=l;r()[b/a7+ a0]=h;r()[b/a7+ _]=g});b.wbg.__wbg_isSafeInteger_f7b04ef02296c4d2=(a=>{const b=Number.isSafeInteger(c(a));return b});b.wbg.__wbindgen_as_number=(a=>{const b=+c(a);return b});b.wbg.__wbg_pathname_5449afe3829f96a1=function(){return F(((b,d)=>{const e=c(d).pathname;const f=o(e,a.__wbindgen_malloc,a.__wbindgen_realloc);const g=l;r()[b/a7+ a0]=g;r()[b/a7+ _]=f}),arguments)};b.wbg.__wbg_search_489f12953342ec1f=function(){return F(((b,d)=>{const e=c(d).search;const f=o(e,a.__wbindgen_malloc,a.__wbindgen_realloc);const g=l;r()[b/a7+ a0]=g;r()[b/a7+ _]=f}),arguments)};b.wbg.__wbg_hash_553098e838e06c1d=function(){return F(((b,d)=>{const e=c(d).hash;const f=o(e,a.__wbindgen_malloc,a.__wbindgen_realloc);const g=l;r()[b/a7+ a0]=g;r()[b/a7+ _]=f}),arguments)};b.wbg.__wbg_history_bc4057de66a2015f=function(){return F((a=>{const b=c(a).history;return k(b)}),arguments)};b.wbg.__wbg_instanceof_Error_e20bb56fd5591a93=(a=>{let b;try{b=c(a) instanceof Z}catch(a){b=!1}const d=b;return d});b.wbg.__wbg_name_e7429f0dda6079e2=(a=>{const b=c(a).name;return k(b)});b.wbg.__wbg_message_5bf28016c2b49cfb=(a=>{const b=c(a).message;return k(b)});b.wbg.__wbg_toString_ffe4c9ea3b3532e9=(a=>{const b=c(a).toString();return k(b)});b.wbg.__wbg_code_5ee5dcc2842228cd=(a=>{const b=c(a).code;return b});b.wbg.__wbg_reason_5ed6709323849cb1=((b,d)=>{const e=c(d).reason;const f=o(e,a.__wbindgen_malloc,a.__wbindgen_realloc);const g=l;r()[b/a7+ a0]=g;r()[b/a7+ _]=f});b.wbg.__wbg_wasClean_8222e9acf5c5ad07=(a=>{const b=c(a).wasClean;return b});b.wbg.__wbg_data_3ce7c145ca4fbcdc=(a=>{const b=c(a).data;return k(b)});b.wbg.__wbg_new_63b92bc8671ed464=(a=>{const b=new $(c(a));return k(b)});b.wbg.__wbg_clearTimeout_541ac0980ffcef74=(a=>{const b=clearTimeout(f(a));return k(b)});b.wbg.__wbg_document_5100775d18896c16=(a=>{const b=c(a).document;return p(b)?_:k(b)});b.wbg.__wbg_setTimeout_7d81d052875b0f4f=function(){return F(((a,b)=>{const d=setTimeout(c(a),b);return k(d)}),arguments)};b.wbg.__wbg_width_ddb5e7bb9fbdd107=(a=>{const b=c(a).width;return b});b.wbg.__wbg_height_2c4b892494a113f4=(a=>{const b=c(a).height;return b});b.wbg.__wbg_readyState_1c157e4ea17c134a=(a=>{const b=c(a).readyState;return b});b.wbg.__wbg_send_70603dff16b81b66=function(){return F(((a,b,d)=>{c(a).send(j(b,d))}),arguments)};b.wbg.__wbg_send_5fcd7bab9777194e=function(){return F(((a,b,d)=>{c(a).send(M(b,d))}),arguments)};b.wbg.__wbg_close_acd9532ff5c093ea=function(){return F((a=>{c(a).close()}),arguments)};b.wbg.__wbg_newwitheventinitdict_c939a6b964db4d91=function(){return F(((a,b,d)=>{const e=new CloseEvent(j(a,b),c(d));return k(e)}),arguments)};b.wbg.__wbg_dispatchEvent_63c0c01600a98fd2=function(){return F(((a,b)=>{const d=c(a).dispatchEvent(c(b));return d}),arguments)};b.wbg.__wbg_removeEventListener_92cb9b3943463338=function(){return F(((a,b,d,e)=>{c(a).removeEventListener(j(b,d),c(e))}),arguments)};b.wbg.__wbg_host_8f1b8ead257c8135=function(){return F(((b,d)=>{const e=c(d).host;const f=o(e,a.__wbindgen_malloc,a.__wbindgen_realloc);const g=l;r()[b/a7+ a0]=g;r()[b/a7+ _]=f}),arguments)};b.wbg.__wbg_new_6c74223c77cfabad=function(){return F(((a,b)=>{const c=new WebSocket(j(a,b));return k(c)}),arguments)};b.wbg.__wbg_setbinaryType_b0cf5103cd561959=((a,b)=>{c(a).binaryType=f(b)});b.wbg.__wbg_getContext_df50fa48a8876636=function(){return F(((a,b,d)=>{const e=c(a).getContext(j(b,d));return p(e)?_:k(e)}),arguments)};b.wbg.__wbg_instanceof_CanvasRenderingContext2d_20bf99ccc051643b=(a=>{let b;try{b=c(a) instanceof CanvasRenderingContext2D}catch(a){b=!1}const d=b;return d});b.wbg.__wbg_setfillStyle_4de94b275f5761f2=((a,b)=>{c(a).fillStyle=c(b)});b.wbg.__wbg_fillRect_b5c8166281bac9df=((a,b,d,e,f)=>{c(a).fillRect(b,d,e,f)});b.wbg.__wbg_self_ce0dbfc45cf2f5be=function(){return F((()=>{const a=self.self;return k(a)}),arguments)};b.wbg.__wbg_window_c6fb939a7f436783=function(){return F((()=>{const a=window.window;return k(a)}),arguments)};b.wbg.__wbg_globalThis_d1e6af4856ba331b=function(){return F((()=>{const a=globalThis.globalThis;return k(a)}),arguments)};b.wbg.__wbg_global_207b558942527489=function(){return F((()=>{const a=global.global;return k(a)}),arguments)};b.wbg.__wbg_newnoargs_e258087cd0daa0ea=((a,b)=>{const c=new Function(j(a,b));return k(c)});b.wbg.__wbg_call_27c0f87801dedf93=function(){return F(((a,b)=>{const d=c(a).call(c(b));return k(d)}),arguments)};b.wbg.__wbg_instanceof_ArrayBuffer_836825be07d4c9d2=(a=>{let b;try{b=c(a) instanceof ArrayBuffer}catch(a){b=!1}const d=b;return d});b.wbg.__wbg_is_010fdc0f4ab96916=((a,b)=>{const d=a8.is(c(a),c(b));return d});b.wbg.__wbg_set_a47bac70306a19a7=((a,b,d)=>{c(a).set(c(b),d>>>_)});b.wbg.__wbg_length_c20a40f15020d68a=(a=>{const b=c(a).length;return b});b.wbg.__wbg_set_1f9b04f170055d33=function(){return F(((a,b,d)=>{const e=Reflect.set(c(a),c(b),c(d));return e}),arguments)};b.wbg.__wbg_error_8e3928cfb8a43e2b=(a=>{console.error(c(a))});b.wbg.__wbg_body_edb1908d3ceff3a1=(a=>{const b=c(a).body;return p(b)?_:k(b)});b.wbg.__wbg_lastChild_83efe6d5da370e1f=(a=>{const b=c(a).lastChild;return p(b)?_:k(b)});b.wbg.__wbg_querySelector_a5f74efc5fa193dd=function(){return F(((a,b,d)=>{const e=c(a).querySelector(j(b,d));return p(e)?_:k(e)}),arguments)};b.wbg.__wbg_href_2edbae9e92cdfeff=((b,d)=>{const e=c(d).href;const f=o(e,a.__wbindgen_malloc,a.__wbindgen_realloc);const g=l;r()[b/a7+ a0]=g;r()[b/a7+ _]=f});b.wbg.__wbg_sethash_9bacb48849d0016e=((a,b,d)=>{c(a).hash=j(b,d)});b.wbg.__wbg_href_7bfb3b2fdc0a6c3f=((b,d)=>{const e=c(d).href;const f=o(e,a.__wbindgen_malloc,a.__wbindgen_realloc);const g=l;r()[b/a7+ a0]=g;r()[b/a7+ _]=f});b.wbg.__wbindgen_error_new=((a,b)=>{const c=new Z(j(a,b));return k(c)});b.wbg.__wbindgen_jsval_loose_eq=((a,b)=>{const d=c(a)==c(b);return d});b.wbg.__wbindgen_boolean_get=(a=>{const b=c(a);const d=typeof b===a3?(b?a0:_):2;return d});b.wbg.__wbindgen_number_get=((a,b)=>{const d=c(b);const e=typeof d===a2?d:V;t()[a/8+ a0]=p(e)?_:e;r()[a/a7+ _]=!p(e)});b.wbg.__wbg_instanceof_Uint8Array_2b3bbecd033d19f6=(a=>{let b;try{b=c(a) instanceof $}catch(a){b=!1}const d=b;return d});b.wbg.__wbindgen_debug_string=((b,d)=>{const e=u(c(d));const f=o(e,a.__wbindgen_malloc,a.__wbindgen_realloc);const g=l;r()[b/a7+ a0]=g;r()[b/a7+ _]=f});b.wbg.__wbindgen_throw=((a,b)=>{throw new Z(j(a,b))});b.wbg.__wbg_then_0c86a60e8fcfe9f6=((a,b)=>{const d=c(a).then(c(b));return k(d)});b.wbg.__wbg_queueMicrotask_481971b0d87f3dd4=(a=>{queueMicrotask(c(a))});b.wbg.__wbg_queueMicrotask_3cbae2ec6b6cd3d6=(a=>{const b=c(a).queueMicrotask;return k(b)});b.wbg.__wbg_resolve_b0083a7967828ec8=(a=>{const b=Promise.resolve(c(a));return k(b)});b.wbg.__wbg_error_696630710900ec44=((a,b,d,e)=>{console.error(c(a),c(b),c(d),c(e))});b.wbg.__wbg_warn_5d3f783b0bae8943=((a,b,d,e)=>{console.warn(c(a),c(b),c(d),c(e))});b.wbg.__wbg_info_80803d9a3f0aad16=((a,b,d,e)=>{console.info(c(a),c(b),c(d),c(e))});b.wbg.__wbg_log_151eb4333ef0fe39=((a,b,d,e)=>{console.log(c(a),c(b),c(d),c(e))});b.wbg.__wbg_debug_7d879afce6cf56cb=((a,b,d,e)=>{console.debug(c(a),c(b),c(d),c(e))});b.wbg.__wbg_performance_3298a9628a5c8aa4=(a=>{const b=c(a).performance;return p(b)?_:k(b)});b.wbg.__wbg_now_4e659b3d15f470d9=(a=>{const b=c(a).now();return b});b.wbg.__wbg_createElement_8bae7856a4bb7411=function(){return F(((a,b,d)=>{const e=c(a).createElement(j(b,d));return k(e)}),arguments)};b.wbg.__wbg_createElementNS_556a62fb298be5a2=function(){return F(((a,b,d,e,f)=>{const g=c(a).createElementNS(b===_?V:j(b,d),j(e,f));return k(g)}),arguments)};b.wbg.__wbg_namespaceURI_5235ee79fd5f6781=((b,d)=>{const e=c(d).namespaceURI;var f=p(e)?_:o(e,a.__wbindgen_malloc,a.__wbindgen_realloc);var g=l;r()[b/a7+ a0]=g;r()[b/a7+ _]=f});b.wbg.__wbg_instanceof_Window_f401953a2cf86220=(a=>{let b;try{b=c(a) instanceof Window}catch(a){b=!1}const d=b;return d});b.wbg.__wbg_putImageData_044c08ad889366e1=function(){return F(((a,b,d,e)=>{c(a).putImageData(c(b),d,e)}),arguments)};b.wbg.__wbg_pushState_b8e8d346f8bb33fd=function(){return F(((a,b,d,e,f,g)=>{c(a).pushState(c(b),j(d,e),f===_?V:j(f,g))}),arguments)};b.wbg.__wbg_pathname_c5fe403ef9525ec6=((b,d)=>{const e=c(d).pathname;const f=o(e,a.__wbindgen_malloc,a.__wbindgen_realloc);const g=l;r()[b/a7+ a0]=g;r()[b/a7+ _]=f});b.wbg.__wbg_new_67853c351755d2cf=function(){return F(((a,b)=>{const c=new URL(j(a,b));return k(c)}),arguments)};b.wbg.__wbg_nextSibling_709614fdb0fb7a66=(a=>{const b=c(a).nextSibling;return p(b)?_:k(b)});b.wbg.__wbg_cloneNode_e19c313ea20d5d1d=function(){return F((a=>{const b=c(a).cloneNode();return k(b)}),arguments)};b.wbg.__wbg_removeChild_96bbfefd2f5a0261=function(){return F(((a,b)=>{const d=c(a).removeChild(c(b));return k(d)}),arguments)};b.wbg.__wbg_href_706b235ecfe6848c=function(){return F(((b,d)=>{const e=c(d).href;const f=o(e,a.__wbindgen_malloc,a.__wbindgen_realloc);const g=l;r()[b/a7+ a0]=g;r()[b/a7+ _]=f}),arguments)};b.wbg.__wbg_setvalue_090972231f0a4f6f=((a,b,d)=>{c(a).value=j(b,d)});b.wbg.__wbg_newwithu8clampedarrayandsh_7f7f549e397591e0=function(){return F(((a,b,c,d)=>{const e=new ImageData(L(a,b),c>>>_,d>>>_);return k(e)}),arguments)};b.wbg.__wbg_checked_749a34774f2df2e3=(a=>{const b=c(a).checked;return b});b.wbg.__wbg_setchecked_931ff2ed2cd3ebfd=((a,b)=>{c(a).checked=b!==_});b.wbg.__wbg_value_47fe6384562f52ab=((b,d)=>{const e=c(d).value;const f=o(e,a.__wbindgen_malloc,a.__wbindgen_realloc);const g=l;r()[b/a7+ a0]=g;r()[b/a7+ _]=f});b.wbg.__wbg_setvalue_78cb4f1fef58ae98=((a,b,d)=>{c(a).value=j(b,d)});b.wbg.__wbg_target_2fc177e386c8b7b0=(a=>{const b=c(a).target;return p(b)?_:k(b)});b.wbg.__wbg_addEventListener_53b787075bd5e003=function(){return F(((a,b,d,e)=>{c(a).addEventListener(j(b,d),c(e))}),arguments)};b.wbg.__wbg_addEventListener_4283b15b4f039eb5=function(){return F(((a,b,d,e,f)=>{c(a).addEventListener(j(b,d),c(e),c(f))}),arguments)};b.wbg.__wbg_instanceof_Element_6945fc210db80ea9=(a=>{let b;try{b=c(a) instanceof Element}catch(a){b=!1}const d=b;return d});b.wbg.__wbg_listenerid_6dcf1c62b7b7de58=((a,b)=>{const d=c(b).__yew_listener_id;r()[a/a7+ a0]=p(d)?_:d;r()[a/a7+ _]=!p(d)});b.wbg.__wbg_setlistenerid_f2e783343fa0cec1=((a,b)=>{c(a).__yew_listener_id=b>>>_});b.wbg.__wbg_parentElement_347524db59fc2976=(a=>{const b=c(a).parentElement;return p(b)?_:k(b)});b.wbg.__wbg_parentNode_6be3abff20e1a5fb=(a=>{const b=c(a).parentNode;return p(b)?_:k(b)});b.wbg.__wbg_instanceof_ShadowRoot_9db040264422e84a=(a=>{let b;try{b=c(a) instanceof ShadowRoot}catch(a){b=!1}const d=b;return d});b.wbg.__wbg_host_c667c7623404d6bf=(a=>{const b=c(a).host;return k(b)});b.wbg.__wbg_composedPath_58473fd5ae55f2cd=(a=>{const b=c(a).composedPath();return k(b)});b.wbg.__wbg_bubbles_abce839854481bc6=(a=>{const b=c(a).bubbles;return b});b.wbg.__wbg_setsubtreeid_e1fab6b578c800cf=((a,b)=>{c(a).__yew_subtree_id=b>>>_});b.wbg.__wbg_setcachekey_75bcd45312087529=((a,b)=>{c(a).__yew_subtree_cache_key=b>>>_});b.wbg.__wbg_cancelBubble_c0aa3172524eb03c=(a=>{const b=c(a).cancelBubble;return b});b.wbg.__wbg_subtreeid_e80a1798fee782f9=((a,b)=>{const d=c(b).__yew_subtree_id;r()[a/a7+ a0]=p(d)?_:d;r()[a/a7+ _]=!p(d)});b.wbg.__wbg_cachekey_b81c1aacc6a0645c=((a,b)=>{const d=c(b).__yew_subtree_cache_key;r()[a/a7+ a0]=p(d)?_:d;r()[a/a7+ _]=!p(d)});b.wbg.__wbg_setinnerHTML_26d69b59e1af99c7=((a,b,d)=>{c(a).innerHTML=j(b,d)});b.wbg.__wbg_childNodes_118168e8b23bcb9b=(a=>{const b=c(a).childNodes;return k(b)});b.wbg.__wbg_from_89e3fc3ba5e6fb48=(a=>{const b=T.from(c(a));return k(b)});b.wbg.__wbg_insertBefore_d2a001abf538c1f8=function(){return F(((a,b,d)=>{const e=c(a).insertBefore(c(b),c(d));return k(e)}),arguments)};b.wbg.__wbg_error_a526fb08a0205972=((b,c)=>{var d=I(b,c).slice();a.__wbindgen_free(b,c*a7,a7);console.error(...d)});b.wbg.__wbg_createTextNode_0c38fd80a5b2284d=((a,b,d)=>{const e=c(a).createTextNode(j(b,d));return k(e)});b.wbg.__wbg_setnodeValue_94b86af0cda24b90=((a,b,d)=>{c(a).nodeValue=b===_?V:j(b,d)});b.wbg.__wbg_value_d7f5bfbd9302c14b=((b,d)=>{const e=c(d).value;const f=o(e,a.__wbindgen_malloc,a.__wbindgen_realloc);const g=l;r()[b/a7+ a0]=g;r()[b/a7+ _]=f});b.wbg.__wbg_setAttribute_3c9f6c303b696daa=function(){return F(((a,b,d,e,f)=>{c(a).setAttribute(j(b,d),j(e,f))}),arguments)};b.wbg.__wbg_removeAttribute_1b10a06ae98ebbd1=function(){return F(((a,b,d)=>{c(a).removeAttribute(j(b,d))}),arguments)};b.wbg.__wbindgen_closure_wrapper1161=((a,b,c)=>{const d=w(a,b,a9,x);return k(d)});b.wbg.__wbindgen_closure_wrapper1250=((a,b,c)=>{const d=w(a,b,a9,A);return k(d)});b.wbg.__wbindgen_closure_wrapper1290=((a,b,c)=>{const d=w(a,b,a9,x);return k(d)});b.wbg.__wbindgen_closure_wrapper1293=((a,b,c)=>{const d=w(a,b,a9,B);return k(d)});b.wbg.__wbindgen_closure_wrapper1297=((a,b,c)=>{const d=w(a,b,a9,B);return k(d)});b.wbg.__wbindgen_closure_wrapper1300=((a,b,c)=>{const d=w(a,b,a9,B);return k(d)});b.wbg.__wbindgen_closure_wrapper3249=((a,b,c)=>{const d=w(a,b,a9,B);return k(d)});b.wbg.__wbindgen_closure_wrapper4164=((a,b,c)=>{const d=C(a,b,a9,D);return k(d)});return b});let a;const b=new T(U).fill(V);b.push(V,W,!0,!1);let d=b.length;const g=typeof TextDecoder!==X?new TextDecoder(Y,{ignoreBOM:!0,fatal:!0}):{decode:()=>{throw Z(`TextDecoder not available`)}};if(typeof TextDecoder!==X){g.decode()};let h=W;let l=_;const m=typeof TextEncoder!==X?new TextEncoder(Y):{encode:()=>{throw Z(`TextEncoder not available`)}};const n=typeof m.encodeInto===a1?((a,b)=>m.encodeInto(a,b)):((a,b)=>{const c=m.encode(a);b.set(c);return {read:a.length,written:c.length}});let q=W;let s=W;const v=typeof a6===X?{register:()=>{},unregister:()=>{}}:new a6(b=>{a.__wbindgen_export_2.get(b.dtor)(b.a,b.b)});let y=U;let G=W;let J=W;export default S;export{R as initSync}