'use strict';
const {Accessory, Service, Characteristic, Categories, HAPStorage, uuid} = require('hap-nodejs');
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
const accessory = new Accessory('Fixture contact', uuid.generate('homecore.fixture.contact'));
const contact = accessory.addService(Service.ContactSensor, 'Fixture contact');
contact.getCharacteristic(Characteristic.ContactSensorState).onGet(() => 1);
const battery = accessory.addService(Service.Battery, 'Fixture battery');
battery.getCharacteristic(Characteristic.BatteryLevel).onGet(() => 69);
battery.getCharacteristic(Characteristic.ChargingState).onGet(() => 0);
battery.getCharacteristic(Characteristic.StatusLowBattery).onGet(() => 0);
accessory.on('listening',port=>{boundPort=port;console.log(`PORT=${port}`)});
accessory.publish({username:'02:11:22:33:44:55',pincode:'123-45-678',category:Categories.SENSOR,port:0,bind:'127.0.0.1',addIdentifyingMaterial:false}).catch(()=>process.exit(1));
process.on('SIGTERM',()=>{accessory.unpublish().finally(()=>process.exit(0))});
