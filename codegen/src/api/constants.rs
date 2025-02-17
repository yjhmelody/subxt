// Copyright 2019-2022 Parity Technologies (UK) Ltd.
// This file is part of subxt.
//
// subxt is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// subxt is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with subxt.  If not, see <http://www.gnu.org/licenses/>.

use crate::types::TypeGenerator;
use frame_metadata::{
    PalletConstantMetadata,
    PalletMetadata,
};
use heck::ToSnakeCase as _;
use proc_macro2::TokenStream as TokenStream2;
use quote::{
    format_ident,
    quote,
};
use scale_info::form::PortableForm;

pub fn generate_constants(
    type_gen: &TypeGenerator,
    pallet: &PalletMetadata<PortableForm>,
    constants: &[PalletConstantMetadata<PortableForm>],
    types_mod_ident: &syn::Ident,
) -> TokenStream2 {
    let constant_fns = constants.iter().map(|constant| {
        let fn_name = format_ident!("{}", constant.name.to_snake_case());
        let pallet_name = &pallet.name;
        let constant_name = &constant.name;
        let return_ty = type_gen.resolve_type_path(constant.ty.id(), &[]);

        quote! {
            pub fn #fn_name(&self) -> ::core::result::Result<#return_ty, ::subxt::BasicError> {
                let pallet = self.client.metadata().pallet(#pallet_name)?;
                let constant = pallet.constant(#constant_name)?;
                let value = ::subxt::codec::Decode::decode(&mut &constant.value[..])?;
                Ok(value)
            }
        }
    });

    quote! {
        pub mod constants {
            use super::#types_mod_ident;

            pub struct ConstantsApi<'a, T: ::subxt::Config> {
                client: &'a ::subxt::Client<T>,
            }

            impl<'a, T: ::subxt::Config> ConstantsApi<'a, T> {
                pub fn new(client: &'a ::subxt::Client<T>) -> Self {
                    Self { client }
                }

                #(#constant_fns)*
            }
        }
    }
}
