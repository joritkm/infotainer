const SOCKETURL = "ws://localhost:8080/ws/"
let state = {
  messages: [],
  socket: null,
  session: null,
}

function connect() {
  disconnect();
  console.log("Connecting...");
  state.socket = new WebSocket(SOCKETURL);
  state.socket.onmessage = (e) => {
    console.log(e.data);
    if (! state.session) {
      state.session = JSON.parse(e.data).content.Response.data
      document.getElementById("clientSessionStatus").textContent = state.session;
    } else {
      state.messages.push(e.data)
      document.getElementById("view").innerHTML += `<p>Server: ` + state.messages[state.messages.length-1] + "</p>";
    }
  }
  state.socket.onclose = () => {
    console.log("Disconnected.")
    state.socket = null;
    state.messages = [];
    state.session = null;
    document.getElementById("clientSessionStatus").textContent = "Not Connected";
    document.getElementById("connectionButton").setAttribute("onclick", "connect()")
    document.getElementById("connectionButton").textContent = "Connect"
    document.getElementById("view").textContent = null
  };
  document.getElementById("connectionButton").setAttribute("onclick", "disconnect()")
  document.getElementById("connectionButton").textContent = "Disconnect"
}

function disconnect() {
  if (state.socket != null) {
    console.log("Disconnecting...");
    state.socket.close()
  }
}

function socket_send() {
  if (state.socket == null) {
    throw "Not connected to a socket"
  }
  valueField = document.getElementById("get")
  const request = valueField.value
  const payload = state.session + "|" + request
  console.log("Sending\t" + payload);
  state.socket.send(payload);
  valueField.value = null
}
