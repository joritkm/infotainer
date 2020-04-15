const TOKENURL = "http://localhost:8080/id";
const SOCKETURL = "ws://localhost:8080/ws/"
const ADD = "add";
const GET = "get";
const DEL = "remove";
const DELIM = "::";
let token = null;
let searchfield = null;
let subfield = null;
let socket = null;


function connect() {
  disconnect();
  console.log("Connecting...");
  socket = new WebSocket(SOCKETURL);
}

function disconnect() {
  if (socket != null) {
    console.log("Disconnecting...");
    socket.onclose = function () {
      console.log("Disconnected.")
      socket = null;
    };
  }
}

function sub_request(subscriptionID, del=false) {
  if (!subscriptionID) {
    subscriptionID = subfield.options[subfield.selectedIndex].value;
  }
  return del ? DEL : ADD + DELIM + subscriptionID
}

function get_request(_data) {
  if (!_data) {
    _data = searchfield.value;
    searchfield.value = "";
  }
  return GET + DELIM + _data
}

function socket_send(_type, _data = null) {
  let payload = null;
  if (_type == GET) {
    payload = get_request(_data);
  } else if (_type == ADD) {
    payload = sub_request(_data);
  } else if (_type == DEL) {
    payload = sub_request(_data, del=true)
  } else {
    throw _type + " is unknown"
  }
  payload = token + "|" + payload;
  if (socket == null) {
    throw "Not connected to a socket"
  }
  console.log("Sending\t" + payload);
  socket.send(payload);
}

document.addEventListener("DOMContentLoaded", async function () {
  searchfield = document.getElementById(GET);
  subfield = document.getElementById(ADD);
  if (!token) {
    let tokenResp = await fetch(TOKENURL);
    token = await tokenResp.text();
  }
  connect();
  socket.onopen= function () {
    socket_send(ADD, "datasets");
  };
});