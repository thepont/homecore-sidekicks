'use strict';
const {Accessory, Bridge, Service, Characteristic, Categories, HAPStorage, uuid} = require('hap-nodejs');
HAPStorage.setCustomStoragePath(process.env.HAP_STORE_DIR);
const {HAPServer}=require('hap-nodejs/dist/lib/HAPServer');
let boundPort=0;
let verifyCount=0;
const verify=HAPServer.prototype.handlePairVerifyM1;
HAPServer.prototype.handlePairVerifyM1=function(connection,request,response,...rest){
  verifyCount++;
  if(verifyCount===Number(process.env.HAP_DROP_VERIFY_NUMBER)){
    response.writeHead(500, {'Content-Length':'0'});response.end();return;
  }
  return verify.call(this,connection,request,response,...rest);
};
const prior=HAPServer.prototype.handleRequestOnHAPConnection;
HAPServer.prototype.handleRequestOnHAPConnection=function(connection,request,response){
  if(request.headers.host !== `127.0.0.1:${boundPort}`){response.writeHead(400, {'Content-Length':'0'});response.end();return;}
  return prior.call(this,connection,request,response);
};
// HAP-NodeJS otherwise binds all IPv4 interfaces even for a loopback address.
const parseBind = Accessory.parseBindOption;
Accessory.parseBindOption = info => ({...parseBind(info), serverAddress:'127.0.0.1'});
// Simulate a storage failure after the accessory has accepted Pair Setup.
const paired=Accessory.prototype.handleInitialPairSetupFinished;
let failStorage=true;
Accessory.prototype.handleInitialPairSetupFinished=function(...args){
  if(failStorage && process.env.HAP_CONTROLLER_DIR){
    require('fs').chmodSync(process.env.HAP_CONTROLLER_DIR,0o500);failStorage=false;
  }
  return paired.apply(this,args);
};
const bridgeMode = process.env.HAP_BRIDGE === '1';
const accessory = bridgeMode ? new Bridge('Fixture bridge', uuid.generate('homecore.fixture.bridge')) : new Accessory('Fixture contact', uuid.generate('homecore.fixture.contact'));
const first = bridgeMode ? new Accessory('First contact', uuid.generate('homecore.fixture.contact')) : accessory;
if (bridgeMode) accessory.addBridgedAccessory(first);
const contact = first.addService(Service.ContactSensor, 'Fixture contact');
let contactValue=1;
const state=contact.getCharacteristic(Characteristic.ContactSensorState);
state.onGet(()=>contactValue);
state.on('subscribe',()=>console.log('SUBSCRIBED'));
require('readline').createInterface({input:process.stdin}).on('line',line=>{
  if(line==='close'){contactValue=0;state.updateValue(contactValue);}
});
const battery = first.addService(Service.Battery, 'Fixture battery');
battery.getCharacteristic(Characteristic.BatteryLevel).onGet(() => 69);
battery.getCharacteristic(Characteristic.ChargingState).onGet(() => 0);
battery.getCharacteristic(Characteristic.StatusLowBattery).onGet(() => 0);
if (bridgeMode) {
  const second = new Accessory('Second contact', uuid.generate('homecore.fixture.second'));
  second.addService(Service.ContactSensor, 'Second contact').getCharacteristic(Characteristic.ContactSensorState).onGet(() => 0);
  accessory.addBridgedAccessory(second);
}
accessory.on('listening',port=>{boundPort=port;console.log(`PORT=${port}`)});
accessory.publish({username:'02:11:22:33:44:55',pincode:'123-45-678',category:Categories.SENSOR,port:0,bind:'127.0.0.1',addIdentifyingMaterial:false}).catch(()=>process.exit(1));
process.on('SIGTERM',()=>{accessory.unpublish().finally(()=>process.exit(0))});
