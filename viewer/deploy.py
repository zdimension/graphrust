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
    with open(r"Z:\web\network5\.htaccess", "w") as f:
        with open(".htaccess", "r") as htaccess:
            f.write(htaccess.read())
        f.write("<FilesMatch \"graph_n4j\\.bin\\.br\">\n")
        f.write(f"Header append X-file-size \"{size}\"\n")
        f.write("</FilesMatch>\n")
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