package fr.plbls.estompe

import android.bluetooth.BluetoothDevice
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService
import androidx.core.content.IntentCompat

/** Tuile des réglages rapides : active ou arrête le mode conduite. */
class DriveTile : TileService() {
  override fun onStartListening() = show(DriveService.running)

  override fun onClick() {
    val on = !DriveService.running
    if (on) DriveService.start(this) else DriveService.stop(this)
    show(on)
  }

  private fun show(on: Boolean) {
    qsTile?.apply { state = if (on) Tile.STATE_ACTIVE else Tile.STATE_INACTIVE; updateTile() }
  }
}

/** Connexion à l'appareil Bluetooth choisi (la voiture) : démarre le mode conduite, et l'arrête à la déconnexion. */
class DriveBluetoothReceiver : BroadcastReceiver() {
  override fun onReceive(context: Context, intent: Intent) {
    val s = DriveStore(context).load()
    if (!s.autoBluetooth || s.bluetooth.isEmpty()) return
    val device = IntentCompat.getParcelableExtra(intent, BluetoothDevice.EXTRA_DEVICE, BluetoothDevice::class.java)
    if (device?.address != s.bluetooth) return
    when (intent.action) {
      BluetoothDevice.ACTION_ACL_CONNECTED -> DriveService.start(context)
      BluetoothDevice.ACTION_ACL_DISCONNECTED -> DriveService.stop(context)
    }
  }
}
