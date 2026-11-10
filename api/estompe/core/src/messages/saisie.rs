use super::*;

pub(super) const SAISIES: usize = 2;
pub(super) const FENETRE_SAISIE: Duration = Duration::from_secs(3);

pub(super) async fn saisie(state: &AppState, u: &User, conv: Conv) -> Result<Response, Response> {
    let pool = pool(state)?;
    let cible = conv
        .channel_id
        .as_deref()
        .or(conv.dm_id.as_deref())
        .unwrap_or_default();
    let cle = format!("{}{cible}", u.id);
    if state
        .limiteurs()
        .saisie
        .allow(&cle, SAISIES, FENETRE_SAISIE)
        && let Ok(to) = participants(pool, &conv).await
    {
        let autres: Vec<String> = to.into_iter().filter(|id| *id != u.id).collect();
        if !autres.is_empty() {
            state.publier(
                &autres,
                "typing",
                &json!({
                    "channel_id": conv.channel_id,
                    "dm_id": conv.dm_id,
                    "user_id": u.id,
                    "pseudo": u.pseudo,
                }),
            );
        }
    }
    Ok(sans_contenu())
}

pub(super) async fn saisie_salon(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
) -> Result<Response, Response> {
    let conv = conv_du_salon(&state, param(&p.0, "cid"), &u.id).await?;
    saisie(&state, &u, conv).await
}

pub(super) async fn saisie_dm(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
) -> Result<Response, Response> {
    let conv = conv_du_dm(pool(&state)?, param(&p.0, "id"), &u.id).await?;
    saisie(&state, &u, conv).await
}
