//! The Markup Layer (Revu's Layers panel > Set as Markup Layer): the layer every new markup is
//! drawn on. New markups take it in [`Session::add_markup`] (their `/OC` is written on save);
//! renaming the layer follows it, deleting it clears it.

use crate::layers::check_name;
use crate::{Result, Session};

impl Session {
    /// The current Markup Layer, if one is set.
    pub fn markup_layer(&self) -> Option<&str> {
        let l = self.doc.markup_layer.as_str();
        (!l.is_empty()).then_some(l)
    }

    /// Set the Markup Layer (created when new), or clear it with `None`.
    pub fn set_markup_layer(&mut self, name: Option<&str>) -> Result<()> {
        match name.map(str::trim).filter(|n| !n.is_empty()) {
            None => {
                self.doc.markup_layer.clear();
                Ok(())
            }
            Some(n) => {
                let n = check_name(n)?;
                if !self.layers().iter().any(|l| l.name == n) {
                    self.create_layer(&n)?;
                }
                self.doc.markup_layer = n;
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use markupcraft_geom::Rect;
    use markupcraft_model::{Kind, Markup};

    use crate::Session;

    fn session() -> Session {
        let bytes = crate::blank::pdf_bytes(&[(612.0, 792.0)]).expect("blank pdf");
        Session::from_bytes(bytes, "ml.pdf").expect("open")
    }

    #[test]
    fn new_markups_go_on_the_markup_layer() {
        let mut s = session();
        let rect = || Markup::new(Kind::Rectangle, 0, Rect::new(10.0, 10.0, 50.0, 50.0).corners().to_vec());
        s.set_markup_layer(Some("Review")).expect("set");
        assert_eq!(s.markup_layer(), Some("Review"));
        assert!(s.layers().iter().any(|l| l.name == "Review"), "created");
        let a = s.add_markup(rect()).expect("add");
        assert_eq!(s.doc().find(&a).map(|m| m.layer.as_str()), Some("Review"));
        let mut own = rect();
        own.layer = "Other".into();
        let b = s.add_markup(own).expect("add");
        assert_eq!(
            s.doc().find(&b).map(|m| m.layer.as_str()),
            Some("Other"),
            "an explicit layer stays"
        );
        s.rename_layer("Review", "Checked").expect("rename");
        assert_eq!(s.markup_layer(), Some("Checked"), "follows a rename");
        s.delete_layer("Checked", false).expect("delete");
        assert_eq!(s.markup_layer(), None, "cleared with its layer");
        let c = s.add_markup(rect()).expect("add");
        assert_eq!(s.doc().find(&c).map(|m| m.layer.as_str()), Some(""));
    }
}
