package fr.plbls.estompe

private val CODE = Regex("[?&]code=")

fun keptAuthUrl(current: String, incoming: String): String =
  if (CODE.containsMatchIn(current) && !CODE.containsMatchIn(incoming)) current else incoming
