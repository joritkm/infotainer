const SOCKETURL = "ws://localhost:8080/ws/"
let state = {
  messages: {},
  socket: null,
  session: null,
}

function connect() {
  disconnect();
  console.log("Connecting...");
  state.socket = new WebSocket(SOCKETURL);
  state.socket.onmessage = (e) => {
    raw = JSON.parse(e.data).data
    console.log(raw);
    if (raw.msg_id === "00000000-0000-0000-0000-000000000000") {
      state.session = raw.data
      document.getElementById("clientSessionStatus").textContent = state.session;
    } else {
      state.messages[raw.msg_id] = raw.data
      document.getElementById("view").innerHTML += `<p>Server: ` + JSON.stringify(state.messages[raw.msg_id]) + "</p>";
    }
  }
  state.socket.onclose = () => {
    console.log("Disconnected.")
    state.socket = null;
    state.messages = {};
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

async function socket_send() {
  if (state.socket == null) {
    throw "Not connected to a socket"
  }
  const reqType = document.getElementById("typeInput").value
  const reqValue = document.getElementById("valueInput").value
  const reqID = {"uid": state.session}
  let reqBody = null
  if (reqType == "List") {
    reqBody = reqType
  } else if (reqType == "Publish") {
    reqBody = { [reqType] :{"param": {"id": reqValue, "data": "Test1312"}}} 
  } else {
    reqBody = { [reqType]: { "param": reqValue } }
  }
  let msgID = (await (await fetch('https://www.uuidgenerator.net/api/version4')).text())
  console.log(msgID)
  const payload = JSON.stringify({ "id": reqID, "msg_id": msgID,"request": reqBody })
  console.log("Sending\n" + payload);
  state.socket.send(payload);
  document.getElementById("valueInput").value = ""
}
