package dev.vellumfe

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Color
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.util.Log
import android.widget.FrameLayout
import android.widget.TextView
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.LifecycleRegistry
import com.google.mlkit.vision.barcode.BarcodeScanning
import com.google.mlkit.vision.barcode.common.Barcode
import com.google.mlkit.vision.common.InputImage
import java.util.concurrent.Executors

/**
 * In-app QR scanner for the character picker. Opens the camera, decodes the
 * first QR it sees, and returns the payload string via [RESULT_TEXT]. Mirrors
 * the iOS shell's `QRScannerView`.
 *
 * A plain Activity is not a LifecycleOwner, so this hosts its own
 * [LifecycleRegistry] to bind CameraX to.
 */
class QrScannerActivity : Activity(), LifecycleOwner {

    private val registry = LifecycleRegistry(this)
    override val lifecycle: Lifecycle get() = registry

    private val analysisExecutor = Executors.newSingleThreadExecutor()
    // getMainExecutor() is API 28; minSdk is 26, so post to the main looper.
    private val mainThreadExecutor = java.util.concurrent.Executor { r ->
        Handler(Looper.getMainLooper()).post(r)
    }
    private val scanner = BarcodeScanning.getClient()
    @Volatile private var delivered = false

    private lateinit var previewView: PreviewView

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        registry.currentState = Lifecycle.State.CREATED

        val root = FrameLayout(this).apply { setBackgroundColor(Color.BLACK) }
        previewView = PreviewView(this)
        root.addView(previewView)
        root.addView(TextView(this).apply {
            text = "Point at a VellumFE pairing QR"
            setTextColor(Color.WHITE)
            setPadding(32, 48, 32, 32)
        })
        setContentView(root)

        if (checkSelfPermission(Manifest.permission.CAMERA) != PackageManager.PERMISSION_GRANTED) {
            requestPermissions(arrayOf(Manifest.permission.CAMERA), CAMERA_REQUEST)
        } else {
            startCamera()
        }
    }

    override fun onRequestPermissionsResult(
        requestCode: Int,
        permissions: Array<out String>,
        grantResults: IntArray,
    ) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode == CAMERA_REQUEST) {
            if (grantResults.firstOrNull() == PackageManager.PERMISSION_GRANTED) {
                startCamera()
            } else {
                finish() // no camera access → back to the picker
            }
        }
    }

    override fun onStart() {
        super.onStart()
        registry.currentState = Lifecycle.State.STARTED
    }

    override fun onResume() {
        super.onResume()
        registry.currentState = Lifecycle.State.RESUMED
    }

    override fun onDestroy() {
        registry.currentState = Lifecycle.State.DESTROYED
        analysisExecutor.shutdown()
        scanner.close()
        super.onDestroy()
    }

    private fun startCamera() {
        val future = ProcessCameraProvider.getInstance(this)
        future.addListener({
            val provider = future.get()
            val preview = Preview.Builder().build().also {
                it.setSurfaceProvider(previewView.surfaceProvider)
            }
            val analysis = ImageAnalysis.Builder()
                .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
                .build()
                .also { it.setAnalyzer(analysisExecutor, ::analyze) }
            try {
                provider.unbindAll()
                provider.bindToLifecycle(this, CameraSelector.DEFAULT_BACK_CAMERA, preview, analysis)
            } catch (e: Exception) {
                Log.w(TAG, "camera bind failed: $e")
                finish()
            }
        }, mainThreadExecutor)
    }

    @androidx.camera.core.ExperimentalGetImage
    private fun analyze(proxy: ImageProxy) {
        val media = proxy.image
        if (media == null || delivered) {
            proxy.close()
            return
        }
        val input = InputImage.fromMediaImage(media, proxy.imageInfo.rotationDegrees)
        scanner.process(input)
            .addOnSuccessListener { codes ->
                val value = codes.firstOrNull { it.valueType == Barcode.TYPE_URL || it.rawValue != null }
                    ?.rawValue
                if (value != null && !delivered) {
                    delivered = true
                    setResult(RESULT_OK, Intent().putExtra(RESULT_TEXT, value))
                    finish()
                }
            }
            .addOnCompleteListener { proxy.close() }
    }

    companion object {
        private const val TAG = "VellumShell"
        private const val CAMERA_REQUEST = 42
        const val RESULT_TEXT = "qr_text"
    }
}
