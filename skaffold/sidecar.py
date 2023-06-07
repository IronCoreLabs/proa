from http.server import BaseHTTPRequestHandler, HTTPServer
import time
from threading import Thread

class Sidecar(BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.send_header("Content-type", "text/plain")
        self.end_headers()
        if self.path == "/shutdown":
            print("Received /shutdown; shutting down.")
            self.wfile.write(bytes("Shutting down.", "utf-8"))
            self.wfile.flush()
            shutdownThread = Thread()
            shutdownThread.run = lambda: self.server.shutdown()
            shutdownThread.start()
        else:
            self.wfile.write(bytes("We're running.", "utf-8"))

print("Sleeping to simulate a slow sidecar startup.")
time.sleep(30)
print("Starting the sidecar.")
server = HTTPServer(("0.0.0.0", 8080), Sidecar)
server.serve_forever()
