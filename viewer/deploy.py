import os
import subprocess

with open("../.cargo/config.toml", "r") as f:
    config = f.read()

STD_CODE = '\n[unstable]\nbuild-std = ["std", "panic_abort"]\n[build]\ntarget-dir = "' + os.path.abspath("../target/web").replace('\\', '\\\\') + '"'

if "build-std" not in config:
    config += STD_CODE
    with open("../.cargo/config.toml", "w") as f:
        f.write(config)

try:
    size = os.path.getsize("../graph_n4j.bin")
    with open("file_size", "w") as f:
        f.write(str(size))
    subprocess.run(["trunk", "build"], check=True)
    subprocess.run(["cmd", "/c", "del", "Z:\\web\\network5\\*", "/Q"], check=True)
    subprocess.run(["xcopy", "dist\\*.*", "Z:\\web\\network5\\", "/s", "/y"], check=True)
    subprocess.run(["xcopy", "assets\\*.*", "Z:\\web\\network5\\", "/s", "/y"], check=True)
    outfile_name = next(f for f in os.listdir("dist") if f.endswith(".wasm"))[:-len("_bg.wasm")]
    with open(r"Z:\web\network5\.htaccess", "w") as f:
        with open(".htaccess", "r") as htaccess:
            f.write(htaccess.read())
        f.write(rf"""
<FilesMatch "graph_n4j\.bin\.br">
    Header append X-file-size "{size}"
</FilesMatch>
<FilesMatch "(index\.html)|(viewer-.*\.js)">
	Header set Pragma "no-cache"
</FilesMatch>
RewriteEngine On
RewriteRule viewer_bg\.wasm$ {outfile_name}_bg.wasm [L]
RewriteCond %{{HTTP_REFERER}} workerHelpers.worker.js$
RewriteRule ^$ {outfile_name}.js [L]
        """)
except subprocess.CalledProcessError as e:
    print(e)
    raise e
finally:
    os.remove("file_size")

    with open("../.cargo/config.toml", "r") as f:
        config = f.read()

    config = config.replace(STD_CODE, '')

    with open("../.cargo/config.toml", "w") as f:
        f.write(config)