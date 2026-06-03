const ort = require('onnxruntime-node');
const modelsDir = "C:\\Users\\gf\\AppData\\Roaming\\com.picasanext.app\\models";

async function testModel(p, label) {
    console.log('[' + label + '] Loading...');
    const t0 = Date.now();
    try {
        const s = await ort.InferenceSession.create(p);
        const ms = Date.now() - t0;
        console.log('[' + label + '] OK in ' + ms + 'ms (' + (ms/1000).toFixed(1) + 's)');
        console.log('[' + label + '] inputs: ' + s.inputNames.join(', '));
    } catch(e) {
        console.log('[' + label + '] FAIL in ' + (Date.now()-t0) + 'ms: ' + e.message.slice(0, 200));
    }
}

(async () => {
    console.log('Node ' + process.version + ' | ORT loaded');
    await testModel(modelsDir + "\\vit-b-16.img.fp16.onnx", "FP16-img");
    await testModel(modelsDir + "\\vit-b-16.txt.fp16.onnx", "FP16-txt");
    console.log("All done.");
    process.exit(0);
})();
