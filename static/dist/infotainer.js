let socket = null;

function connect() {
  disconnect();
  console.log("Connecting...");
  socket = new WebSocket("ws://localhost:8080/ws/");
  socket.onopen = function() {
    console.log("Connected.");

  }
}

function disconnect() {
  if (socket != null) {
    console.log("Disconnecting...");
    socket.close();
    socket = null;
  }
}
