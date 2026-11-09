# SpielOrders

SpielOrders creates, inspects, and maintains administrative Orders together with their Supporting Material. Each Order retains one continuous identity throughout preparation, placement, receipt, and completion.

## Language

**Reusable Free-Entry Value**:
An optional textual classification chosen from previously used values or explicitly confirmed as new. Matching trims surrounding whitespace and ignores letter case, reusing the existing canonical spelling when matched.
_Avoid_: Fixed enumeration, unrestricted duplicate label

**Order**:
A single administrative purchasing effort whose facts and Supporting Material are managed together, whether it is still being prepared or was placed previously.
_Avoid_: Purchase record, transaction

**Order ID**:
The immutable opaque identifier assigned to one Order when SpielOrders adopts or creates it. An Order keeps this identity when its facts or Order Directory name change.
_Avoid_: Order Directory path, Order Title, filename

**Order Title**:
The user-editable description that identifies what an Order is for.
_Avoid_: Description, brief, Order name

**Date Ordered**:
The user-editable calendar date used as an Order's archival ordering date and the only lifecycle date recorded by SpielOrders. It may be provisional while the Order is `In Preparation` or on `Hold`; month-level accuracy is sufficient for archival use.
_Avoid_: Exact purchase timestamp, Filing Date, creation date

**Orderer**:
The optional Reusable Free-Entry Value naming the person for whom an Order is principally being tracked. It is not an account or purchasing-system actor.
_Avoid_: Requestor, user, purchaser

**Order Directory**:
An ordinary direct-child directory of the configured Orders Directory, associated with one Order. Its name may differ from the current suggested projection without constituting damage, and its Supporting Material requires no particular internal structure.
_Avoid_: Order Bucket, Order folder, record directory

**SpielOrders Root**:
The user-chosen filesystem directory at the root of one SpielOrder Package. It remains available for the user's other material.
_Avoid_: Project, database directory

**Orders Directory**:
The configured directory containing a SpielOrder Package's Order Directories. It defaults to `Orders` within the SpielOrders Root.
_Avoid_: All Orders, Orders Root

**Removed Orders Directory**:
The configured directory outside the Orders Directory where SpielOrders preserves complete Order Directories that a user has removed from active management. Its contents are not active Orders and are not discovered, indexed, searched, validated, upgraded, or repaired by SpielOrders.
_Avoid_: Trash, archive, deleted Orders

**SpielOrder Package**:
A filesystem directory managed by SpielOrders, including its Orders and shared administrative information.
_Avoid_: Project, SpielOrders Project, collection

**Package Index**:
A rebuildable summary of the current Orders and Vendors used to load a SpielOrder Package efficiently. It contains no canonical domain fact and may be replaced from canonical records.
_Avoid_: Order database, canonical record

**Package README**:
The conventional, user-authored `README.md` at the SpielOrders Root. SpielOrders always presents it for editing, treats an absent file as empty, and contains no programmatically determined content.
_Avoid_: Project README, generated index

**Order Item**:
One good or service included in an Order, with a required Description, Quantity, and Unit Price and an optional Catalog Number. Order Items belong to their Order and are not independent records.
_Avoid_: Order Data, product row

**Item Total Price**:
The unrounded floating-point result of multiplying an Order Item's Quantity by its Unit Price. Rounding applies only when the value is displayed.
_Avoid_: Order Total, Line Total

**Order Total**:
The unrounded sum of an Order's Item Total Prices. Fees, shipping, taxes, and discounts are represented as ordinary Order Items rather than separate adjustments.
_Avoid_: Manually entered total, Item Total Price

**Order README**:
The conventional, user-authored `README.md` within an Order Directory. SpielOrders always presents it for editing, treats an absent file as empty, and contains no programmatically determined content.
_Avoid_: Notes, generated summary

**Vendor**:
The person or organization from which an Order is obtained. A Vendor requires one Vendor Name and may retain general phone, email, website, multiline address, Vendor Contacts, and purchasing notes; it is shared across Orders and has one continuous identity despite changes to its facts. A Vendor is never deleted: taking one out of use is a visibility change that makes it a Hidden Vendor, so its record and Vendor ID survive and every Order referencing it keeps resolving.
_Avoid_: Supplier record

**Vendor ID**:
The immutable opaque identifier assigned to one Vendor. A Vendor keeps this identity when its name or other shared facts change.
_Avoid_: Vendor name, legal name, vendors.json position

**Vendor Reference**:
An Order's optional, user-editable association to one Vendor by Vendor ID at any Order Status. The reference always resolves to the Vendor's current shared facts; an Order retains no historical Vendor snapshot.
_Avoid_: Vendor Name reference, vendors.json position, Order Directory component

**Unresolved Vendor Reference**:
A Vendor Reference whose Vendor ID is absent from the SpielOrder Package's Vendors. It is an integrity failure, not an unknown Vendor, and never authorizes inferring or creating a Vendor from an Order Directory name.
_Avoid_: Unknown Vendor, missing optional Vendor

**Vendor Name**:
The single required, nonblank, user-editable name used to identify a Vendor throughout one SpielOrder Package. Vendor Names are unique after trimming surrounding whitespace and ignoring letter case.
_Avoid_: Display name, legal name, Vendor alias

**Hidden Vendor**:
A Vendor that SpielOrders no longer offers for new use, while its record and Vendor ID remain and every Order referencing it resolves its current shared facts exactly as before. Hiding is a reversible toggle and is permitted whether or not Orders reference the Vendor; it governs what is offered, never what is resolved.
_Avoid_: Disabled Vendor, deleted Vendor, archived Vendor, inactive Vendor

**Vendor Contact**:
A named person associated with a Vendor, with optional role, phone, and email information. A Vendor may retain any number of Vendor Contacts independently of its general contact information.
_Avoid_: Orderer, general Vendor email

**Order Status**:
The user-assigned stage an Order has reached: `In Preparation`, `Hold`, `Ordered`, `Received`, or `Complete`. Any status may change directly to any other without retaining transition history; SpielOrders does not infer status from deliveries, Order Items, or metadata completeness.
_Avoid_: State, institution status

**Ordering Institution**:
The optional Reusable Free-Entry Value naming the institution through which an Order is placed, such as Northwind, Uplands, or Cedarbrook.
_Avoid_: Order location, status

**Order Currency**:
The optional three-letter ISO currency-code decoration shared by every monetary amount in one Order. Changing or omitting it never converts or modifies numeric values; each front end chooses the precision it displays them with.
_Avoid_: Exchange rate, converted price

**Project**:
The optional Reusable Free-Entry Value naming the scientific or administrative effort principally associated with an Order; an Order cannot name more than one.
_Avoid_: Order Group, tag

**Order Group**:
An optional Reusable Free-Entry Value for collecting and filtering related Orders; an Order cannot name more than one, and SpielOrders does not prescribe what the grouping means.
_Avoid_: Project, Function

**Supporting Material**:
The opaque user-owned content in an Order Directory other than its reserved `.spielorders` artifacts and conventional Order README. It may have any internal filesystem structure and remains outside SpielOrders validation and repair authority.
_Avoid_: Attachment

**Damaged Package**:
A SpielOrder Package whose package-wide canonical artifacts cannot be trusted. It remains available for diagnosis, but ordinary changes are not allowed until package-wide integrity is restored.
_Avoid_: Unmanaged directory, empty package

**Damaged Order**:
An Order Directory whose canonical Order facts cannot be trusted or whose Order identity conflicts with another Order. It is quarantined from ordinary Order use without preventing healthy Orders in the same SpielOrder Package from being used.
_Avoid_: Invalid Supporting Material, deleted Order

**Metadata Upgrade**:
A coordinated transformation of every SpielOrders-managed metadata artifact in a SpielOrder Package from an older supported contract to a newer one. It preserves domain meaning and is distinct from repairing damaged metadata.
_Avoid_: Repair, per-file migration, normalization
