use crate::app::{iter_progress, Cancelable, ModularityClass, Person, StatusWriter, StringTables, ViewerData};

use graph_format::{EdgeStore, GraphFile};
use itertools::Itertools;
use rayon::prelude::*;

use speedy::Readable;

use crate::utils::{str_from_null_terminated_utf8, SliceExt};

use crate::{for_progress, log, log_progress};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

//const GRAPH_NAME: &str = "graph2.bin";
const GRAPH_NAME: &str = "graph_n4j.bin";
//const GRAPH_NAME: &str = "graph_n4j_5.57M_400k.bin";

#[cfg(not(target_arch = "wasm32"))]
pub fn load_file(_status_tx: &StatusWriter) -> Cancelable<GraphFile> {
    GraphFile::read_from_file(format!("{}/../{}", env!("CARGO_MANIFEST_DIR"), GRAPH_NAME))
        .map_err(Into::into)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(inline_js = "export function downloadGraph(filesize, progressHandler) {
    const DB_NAME = 'graphCacheDB';
    const DB_VERSION = 1;
    const STORE_NAME = 'files';

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
                reject('No IndexedDB available');
                return;
            }

            const transaction = db.transaction([STORE_NAME], 'readonly');
            const store = transaction.objectStore(STORE_NAME);
            const request = store.get('graph_n4j.bin.br');

            request.onsuccess = event => {
                const file = event.target.result;
                if (file && file.size === filesize) {
                    console.log('File found with correct size');
                    resolve(file.data);
                } else {
                    console.log('File not found or size mismatch');
                    resolve(null); // Return null if file not found or size mismatch
                }
            };

            request.onerror = event => {
                reject('Error retrieving file from IndexedDB: ' + event.target.errorCode);
            };
        });
    }

    // Download file and cache it in IndexedDB
    function fetchAndCacheFile(db, filesize, progressHandler) {
        return fetch('graph_n4j.bin.br?size=' + filesize, {
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
                    // Store the downloaded file in IndexedDB
                    const transaction = db.transaction([STORE_NAME], 'readwrite');
                    const store = transaction.objectStore(STORE_NAME);
                    const request = store.put({
                        id: 'graph_n4j.bin.br',
                        size: filesize,
                        data: arrayBuffer
                    });

                    request.onsuccess = () => {
                        console.log('File cached in IndexedDB');
                    };

                    request.onerror = event => {
                        console.error('Error caching file in IndexedDB: ' + event.target.errorCode);
                    };
                }
                return arrayBuffer;
            });
    }
}")]
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
            ignore_error!(log_progress!(status_tx_, percent, 100));
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
    use crate::app::StatusWriterInterface;
    let progress_handler = Closure::wrap(Box::new(move |progress: usize| {
        status_tx_.send(crate::app::Progress { max: 100, val: progress }).unwrap()
    }) as Box<dyn FnMut(usize)>);
    js_console_log("Awaiting JS promise");
    let result = wasm_bindgen_futures::JsFuture::from(downloadGraph(include_str!("../file_size").parse().unwrap(), progress_handler.as_ref().unchecked_ref()))
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

pub fn load_binary(status_tx: &StatusWriter, content: GraphFile) -> Cancelable<ProcessedData> {
    log!(status_tx, "Binary content loaded");
    log!(status_tx, "Class count: {}", content.class_count);
    log!(status_tx, "Node count: {}", content.node_count);
    //log!(status_tx, "Edge count: {}", content.edge_count);

    log!(status_tx, "Processing modularity classes");

    let modularity_classes = content
        .classes
        .iter()
        .copied()
        .enumerate()
        .map(|(id, color)| ModularityClass::new(color, id as u16))
        .collect_vec();

    log!(status_tx, "Processing nodes");

    let start = chrono::Local::now();
    let mut person_data: Vec<_> = iter_progress(content.nodes.iter(), status_tx)
        .map(|node| unsafe {
            Person::new(
                node.position,
                node.size,
                node.class,
                str_from_null_terminated_utf8(content.ids.as_ptr().offset(node.offset_id as isize)),
                str_from_null_terminated_utf8(
                    content.names.as_ptr().offset(node.offset_name as isize),
                ),
                node.total_edge_count as usize,
            )
        })
        .collect();

    log!(
        status_tx,
        "Done, took {}ms",
        (chrono::Local::now() - start).num_milliseconds()
    );

    log!(status_tx, "Generating neighbor lists");

    let start = chrono::Local::now();
    /*let how_often = (content.edges.len() / 100).max(1);
    for (i, edge) in content.edges.iter().enumerate() {
        if edge.a == edge.b {
            //panic!("Self edge detected"); TODO
            continue;
        }
        let (p1, p2) = person_data.get_two_mut(edge.a as usize, edge.b as usize);
        p1.neighbors.push(edge.b as usize);
        p2.neighbors.push(edge.a as usize);
        if i % how_often == 0 {
            log_progress!(status_tx, i, content.edges.len());
        }
    }*/

    /*for_progress!(status_tx, edge in content.edges.iter(), {
        if edge.a == edge.b {
            //panic!("Self edge detected"); TODO
            continue;
        }
        let (p1, p2) = person_data.get_two_mut(edge.a as usize, edge.b as usize);
        p1.neighbors.push(edge.b as usize);
        p2.neighbors.push(edge.a as usize);
    });*/

    let mut edges = Vec::new();

    for_progress!(status_tx, (i, n) in content.nodes.iter().enumerate(), {
        edges.reserve(n.edge_count as usize);
        for e in n.edges.iter().copied() {
            person_data[i].neighbors.push(e as usize);
            person_data[e as usize].neighbors.push(i);
            edges.push(EdgeStore {
                a: i as u32,
                b: e,
            });
        }
    });

    log!(
        status_tx,
        "Done, took {}ms",
        (chrono::Local::now() - start).num_milliseconds()
    );

    Ok(ProcessedData {
        strings: StringTables {
            ids: content.ids,
            names: content.names,
        },
        viewer: ViewerData::new(person_data, modularity_classes, status_tx)?,
        edges: edges,
    })
}
