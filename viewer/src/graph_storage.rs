use crate::app::{iter_progress, ModularityClass, Person, StringTables, ViewerData};

use graph_format::{EdgeStore, GraphFile};
use itertools::Itertools;
use rayon::prelude::*;

use speedy::Readable;

use crate::utils::{str_from_null_terminated_utf8, SliceExt};

use crate::threading::{Cancelable, StatusWriter, StatusWriterInterface};
use crate::{for_progress, log};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

//const GRAPH_NAME: &str = "graph2.bin";
const GRAPH_NAME: &str = "graph_n4j.bin";
//const GRAPH_NAME: &str = "graph_n4j_5.57M_400k.bin";

#[cfg(not(target_arch = "wasm32"))]
pub fn load_file(_status_tx: &impl StatusWriterInterface) -> Cancelable<GraphFile> {
    GraphFile::read_from_file(format!("{}/../{}", env!("CARGO_MANIFEST_DIR"), GRAPH_NAME))
        .map_err(Into::into)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(
    inline_js = "export function downloadGraph(filesize, progressHandler) {
    const DB_NAME = 'graphCacheDB';
    const DB_VERSION = 2;
    const STORE_NAME = 'files';
    const FILE_NAME = 'graph_n4j.bin.br';

    // Open the IndexedDB
    return openIndexedDB().then(db => {
        return getFileFromDB(db, filesize)
            .then(cachedFile => {
                if (cachedFile) {
                    // If file is already in the cache and matches the size, return it
                    return cachedFile;
                } else {
                    // If not cached or size mismatch, download and cache the file
                    return fetchAndCacheFile(db, filesize, progressHandler);
                }
            })
            .catch(() => {
                // If any error occurs while checking the cache, fall back to download
                return fetchAndCacheFile(null, filesize, progressHandler);
            });
    }).catch(() => {
        // If any error occurs when opening the IndexedDB, fall back to download
        return fetchAndCacheFile(null, filesize, progressHandler);
    });

    // Open IndexedDB and create object store if needed
    function openIndexedDB() {
        return new Promise((resolve, reject) => {
            const request = indexedDB.open(DB_NAME, DB_VERSION);

            request.onupgradeneeded = event => {
                const db = event.target.result;
                if (!db.objectStoreNames.contains(STORE_NAME)) {
                    db.createObjectStore(STORE_NAME, { keyPath: 'id' });
                }
            };

            request.onsuccess = event => {
                resolve(event.target.result);
            };

            request.onerror = event => {
                reject('Error opening IndexedDB: ' + event.target.errorCode);
            };
        });
    }

    // Get file from IndexedDB
    function getFileFromDB(db, filesize) {
        return new Promise((resolve, reject) => {
            if (!db) {
                return reject('No IndexedDB available');
            }
    
            const transaction = db.transaction([STORE_NAME], 'readonly');
            const store = transaction.objectStore(STORE_NAME);
            const metaRequest = store.get(FILE_NAME);
    
            metaRequest.onsuccess = event => {
                const meta = event.target.result;
                if (meta && meta.size === filesize && meta.parts) {
                    const parts = new Array(meta.parts).fill(null).map((_, i) => {
                        return new Promise((resolve, reject) => {
                            const partRequest = store.get(`${FILE_NAME}_part${i}`);
                            partRequest.onsuccess = event => {
                                const data = event.target.result.data;
                                if (!data || data == {}) {
                                    console.warn(`Part ${i} not found in IndexedDB`);
                                    resolve(null);
                                } else {
                                    resolve(event.target.result.data);
                                }
                            };
                            partRequest.onerror = event => {
                                reject(`Error retrieving part ${i} from IndexedDB: ${event.target.errorCode}`);
                            };
                        });
                    });
    
                    Promise.all(parts).then(chunks => {
                        const fileData = new Uint8Array(filesize);
                        let offset = 0;
                        for (const chunk of chunks) {
                            if (!chunk) {
                                console.log('Part not found');
                                resolve(null);
                                return;
                            }
                            fileData.set(new Uint8Array(chunk), offset);
                            offset += chunk.byteLength;
                        }
                        resolve(fileData.buffer);
                    }).catch(reject);
                } else {
                    console.log('File not found or size mismatch');
                    resolve(null); // Return null if file not found or size mismatch
                }
            };
    
            metaRequest.onerror = event => {
                reject(`Error retrieving metadata from IndexedDB: ${event.target.errorCode}`);
            };
        });
    }

    // Download file and cache it in IndexedDB
    function fetchAndCacheFile(db, filesize, progressHandler) {
        return fetch(FILE_NAME + '?size=' + filesize, {
                cache: 'force-cache',
                headers: {
                    'Cache-Control': 'max-age=31536000',
                    'Accept-Encoding': 'br'
                }
            })
            .then(response => {
                if (!response.ok) {
                    throw Error(response.status + ' ' + response.statusText);
                }

                const contentLength = response.headers.get('x-file-size');
                if (contentLength === null) {
                    throw Error('Response size header unavailable');
                }

                const total = parseInt(contentLength, 10);
                let loaded = 0;
                let progress = 0;

                return new Response(
                    new ReadableStream({
                        start(controller) {
                            const reader = response.body.getReader();

                            read();

                            function read() {
                                reader.read().then(({ done, value }) => {
                                    if (done) {
                                        controller.close();
                                        return;
                                    }
                                    loaded += value.byteLength;
                                    let newProgress = Math.round(loaded / total * 100);
                                    if (newProgress > progress) {
                                        progress = newProgress;
                                        progressHandler(progress);
                                    }
                                    controller.enqueue(value);
                                    read();
                                }).catch(error => {
                                    console.error(error);
                                    controller.error(error);
                                });
                            }
                        }
                    })
                );
            })
            .then(a => a.arrayBuffer())
            .then(arrayBuffer => {
                if (db) {
                    const CHUNK_SIZE = 200 * 1024 * 1024; // 200MB, because Firefox has a limit
                    const totalParts = Math.ceil(arrayBuffer.byteLength / CHUNK_SIZE);

                    try {        
                        for (let i = 0; i < totalParts; i++) {
                            const transaction = db.transaction([STORE_NAME], 'readwrite');
                            const store = transaction.objectStore(STORE_NAME);
                            const start = i * CHUNK_SIZE;
                            const end = Math.min(start + CHUNK_SIZE, arrayBuffer.byteLength);
                            const chunk = arrayBuffer.slice(start, end);
                
                            const request = store.put({
                                id: `${FILE_NAME}_part${i}`,
                                data: chunk
                            });
                
                            request.onsuccess = () => {
                                console.log(`Part ${i} cached in IndexedDB`);
                            };
                
                            request.onerror = event => {
                                console.error(`Error caching part ${i} in IndexedDB: ` + event.target.errorCode);
                            };
                        }

                        const transaction = db.transaction([STORE_NAME], 'readwrite');
                        const store = transaction.objectStore(STORE_NAME);
                
                        const metaRequest = store.put({
                            id: FILE_NAME,
                            size: arrayBuffer.byteLength,
                            parts: totalParts
                        });
                
                        metaRequest.onsuccess = () => {
                            console.log('Metadata cached in IndexedDB');
                        };
                
                        metaRequest.onerror = event => {
                            console.error('Error caching metadata in IndexedDB: ' + event.target.errorCode);
                        };
                    } catch (error) {
                        console.error('Error caching file in IndexedDB: ' + error);
                    }
                }
                return arrayBuffer;
            });
    }
}"
)]
extern "C" {
    fn downloadGraph(filesize: u32, progress: &js_sys::Function) -> js_sys::Promise;
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    // Use `js_namespace` here to bind `console.log(..)` instead of just
    // `log(..)`
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn js_console_log(s: &str);
}

#[cfg(target_arch = "wasm32")]
pub async fn load_file(status_tx: &StatusWriter) -> Cancelable<GraphFile> {
    /*let url = "https://domino.zdimension.fr/web/network5/graph_n4j.bin.br";
    let xhr = web_sys::XmlHttpRequest::new().unwrap();
    xhr.open("GET", url).unwrap();
    xhr.set_response_type(web_sys::XmlHttpRequestResponseType::Arraybuffer);
    let status_tx_ = status_tx.clone();
    xhr.set_onprogress(Some(Closure::wrap(Box::new(move |e: web_sys::ProgressEvent| {
        if e.length_computable() {
            let percent = (e.loaded() as f64 / e.total() as f64 * 100.0).round() as usize;
            let _ = try_log_progress!(status_tx_, percent, 100);
        }
    }) as Box<dyn FnMut(_)>).as_ref().unchecked_ref()));
    let prom = js_sys::Promise::new(&mut move |resolve, reject| {
        let reject_ = reject.clone();
        let xhr_ = xhr.clone();
        let closure = Closure::wrap(Box::new(move || {
            if xhr_.status() == Ok(200) {
                // return xhr response
                let array_buffer = xhr_.response().unwrap();
                resolve.call0(&array_buffer).unwrap();
            } else {
                reject_.call0(&JsValue::NULL).unwrap();
            }
        }) as Box<dyn FnMut()>);
        xhr.set_onload(Some(closure.as_ref().unchecked_ref()));
        closure.forget();
        let closure = Closure::wrap(Box::new(move || {
            reject.call0(&JsValue::NULL).unwrap();
        }) as Box<dyn FnMut()>);
        xhr.set_onerror(Some(closure.as_ref().unchecked_ref()));
        closure.forget();
        xhr.send().unwrap();
    });
    let future = wasm_bindgen_futures::JsFuture::from(prom);
    log!(status_tx, "Starting download");
    let res = future.await.unwrap();
    log!(status_tx, "Finished");
    let array_buffer = res;
    let vec = js_sys::Uint8Array::new(&array_buffer).to_vec();
    GraphFile::read_from_buffer(&vec).map_err(Into::into)*/

    let global = js_sys::global().unchecked_into::<web_sys::WorkerGlobalScope>();
    // function downloadFile(progress)
    /*let download_file_fn = global
    .get("downloadFile")
    .unwrap()
    .dyn_into::<js_sys::Function>()
    .unwrap();*/
    log!(status_tx, "Downloading file");
    let status_tx_ = status_tx.clone();
    use crate::threading::StatusWriterInterface;
    let progress_handler = Closure::wrap(Box::new(move |progress: usize| {
        status_tx_
            .send(crate::threading::Progress {
                max: 100,
                val: progress,
            })
            .unwrap()
    }) as Box<dyn FnMut(usize)>);
    js_console_log("Awaiting JS promise");
    let result = wasm_bindgen_futures::JsFuture::from(downloadGraph(
        include_str!("../file_size").parse().unwrap(),
        progress_handler.as_ref().unchecked_ref(),
    ))
    .await
    .unwrap();
    js_console_log("Converting to Uint8Array");
    let array_buffer = js_sys::Uint8Array::new(&result);
    js_console_log("Converting to Vec");
    let array_buffer = array_buffer.to_vec();
    js_console_log("Decoding to GraphFile object");
    let f = GraphFile::read_from_buffer(&array_buffer).map_err(Into::into);
    js_console_log("File read end");
    log!(status_tx, "File read");
    f
}

pub struct ProcessedData {
    pub strings: StringTables,
    pub viewer: ViewerData,
    pub edges: Vec<EdgeStore>,
}

pub fn load_binary(
    status_tx: &impl StatusWriterInterface,
    content: GraphFile,
) -> Cancelable<ProcessedData> {
    log!(status_tx, t!("Binary content loaded"));
    log!(
        status_tx,
        t!("Class count: %{count}", count = content.classes.len())
    );
    log!(
        status_tx,
        t!("Node count: %{count}", count = content.nodes.len())
    );
    //log!(status_tx, "Edge count: {}", content.edge_count);

    log!(status_tx, t!("Processing modularity classes"));

    let modularity_classes = content
        .classes
        .iter()
        .copied()
        .enumerate()
        .map(|(id, color)| ModularityClass::new(color, id as u16))
        .collect_vec();

    log!(status_tx, t!("Processing nodes"));

    let start = chrono::Local::now();
    let mut neighbor_lists: Vec<_> = iter_progress(content.nodes.iter(), status_tx)
        .map(|node| Vec::with_capacity(node.total_edge_count as usize))
        .collect();
    let mut person_data: Vec<_> = iter_progress(content.nodes.iter(), status_tx)
        .map(|node| {
            Person::new(
                node.position,
                node.size,
                node.class,
                // SAFETY: the strings are null-terminated
                unsafe {
                    str_from_null_terminated_utf8(
                        content.ids.as_ptr().offset(node.offset_id as isize),
                    )
                },
                unsafe {
                    str_from_null_terminated_utf8(
                        content.names.as_ptr().offset(node.offset_name as isize),
                    )
                },
            )
        })
        .collect();

    log!(
        status_tx,
        t!(
            "Done, took %{time}ms",
            time = (chrono::Local::now() - start).num_milliseconds()
        )
    );

    log!(status_tx, t!("Generating neighbor lists"));

    let start = chrono::Local::now();

    let mut edges = Vec::new();

    for_progress!(status_tx, (i, n) in content.nodes.iter().enumerate(), {
        edges.reserve(n.edge_count as usize);
        for e in n.edges.iter().copied() {
            neighbor_lists[i].push(e as usize);
            neighbor_lists[e as usize].push(i);
            edges.push(EdgeStore {
                a: i as u32,
                b: e,
            });
        }
    });

    log!(status_tx, t!("Associating neighbor lists"));

    for_progress!(status_tx, (person, nblist) in person_data.iter_mut().zip(neighbor_lists.iter()), {
        // SAFETY: neighbor_lists is kept alive
        person.neighbors = unsafe { std::mem::transmute(nblist.as_slice()) };
    });

    log!(
        status_tx,
        t!(
            "Done, took %{time}ms",
            time = (chrono::Local::now() - start).num_milliseconds()
        )
    );

    Ok(ProcessedData {
        strings: StringTables {
            ids: content.ids,
            names: content.names,
        },
        viewer: ViewerData::new(person_data, neighbor_lists, modularity_classes)?,
        edges,
    })
}
