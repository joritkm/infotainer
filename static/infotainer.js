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
      state.messages.push(JSON.parse(e.data).content)
      document.getElementById("view").innerHTML += `<p>Server: ` + JSON.stringify(state.messages[state.messages.length-1]) + "</p>";
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
  const reqType = document.getElementById("typeInput").value
  let reqValue = document.getElementById("valueInput").value
  const reqID = {"uid": state.session}
  let reqBody = null
  console.log(reqType)
  if (reqType == "List") {
    reqBody = reqType
  } else if (reqType == "Publish") {
    reqBody = { [reqType] :{"param": {"id": reqValue, "data": "Test1312"}}} 
  } else {
    reqBody = { [reqType]: { "param": reqValue} }
  }
  const payload = JSON.stringify({ "id": reqID, "request": reqBody })
  console.log("Sending\t" + payload);
  state.socket.send(payload);
  document.getElementById("valueInput").value = ""
}
