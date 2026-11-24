import { Hono } from 'hono';
import { z } from 'zod';
import { catalogClient } from '../clients/catalogClient.js';
import type { RequestContext } from '../clients/http.js';
import type {
  CreateCatalogItemCommand,
  Criteria,
  ItemDetails,
  UpdateCatalogItemCommand,
} from '../types/platform.js';

const createCategorySchema = z.object({
  parent_id: z.string().uuid().optional().nullable(),
  name: z.string().trim().min(1, 'El nombre de la categoría es obligatorio'),
  slug: z
    .string()
    .trim()
    .min(1, 'El slug es obligatorio')
    .regex(/^[a-z0-9-]+$/, 'El slug solo puede contener minúsculas, números y guiones'),
  description: z.string().optional().nullable(),
  display_order: z.number().int().optional(),
  is_active: z.boolean().optional(),
});

const itemDetailsSchema = z.discriminatedUnion('item_type', [
  z.object({
    item_type: z.literal('SERVICE'),
    duration_minutes: z.coerce.number().int().positive('duration_minutes debe ser > 0'),
    buffer_minutes: z.coerce.number().int().min(0).optional().default(0),
  }),
  z.object({
    item_type: z.literal('PRODUCT'),
    unit_of_measure: z.string().optional().nullable(),
    track_inventory: z.boolean().optional().default(false),
  }),
  z.object({
    item_type: z.literal('BUNDLE'),
    items: z
      .array(
        z.object({
          item_id: z.string().uuid(),
          quantity: z.coerce.number().int().positive(),
        }),
      )
      .optional()
      .default([]),
  }),
]);

const baseItemFields = {
  category_id: z.string().uuid().optional().nullable(),
  sku: z.string().optional().nullable(),
  name: z.string().trim().min(1, 'El nombre del ítem es obligatorio'),
  description: z.string().optional().nullable(),
  base_price: z.coerce.number().int().min(0, 'base_price debe ser >= 0'),
  currency: z.string().length(3).optional().default('CLP'),
  tax_rate_bps: z.coerce.number().int().min(0).max(10000).optional(),
  tax_included: z.boolean().optional(),
  is_active: z.boolean().optional(),
  metadata: z.record(z.unknown()).optional(),
};

const createItemSchema = z.intersection(z.object(baseItemFields), itemDetailsSchema);

const updateItemSchema = z.object({
  category_id: z.string().uuid().optional().nullable(),
  sku: z.string().optional().nullable(),
  name: z.string().trim().min(1).optional(),
  description: z.string().optional().nullable(),
  base_price: z.coerce.number().int().min(0).optional(),
  currency: z.string().length(3).optional(),
  tax_rate_bps: z.coerce.number().int().min(0).max(10000).optional(),
  tax_included: z.boolean().optional(),
  details: itemDetailsSchema.optional(),
  // Also accept flattened fields from clients and pack them into `details` for catalog-api
  item_type: z.enum(['SERVICE', 'PRODUCT', 'BUNDLE']).optional(),
  duration_minutes: z.coerce.number().int().positive().optional(),
  buffer_minutes: z.coerce.number().int().min(0).optional(),
  unit_of_measure: z.string().optional().nullable(),
  track_inventory: z.boolean().optional(),
  items: z
    .array(
      z.object({
        item_id: z.string().uuid(),
        quantity: z.coerce.number().int().positive(),
      }),
    )
    .optional(),
  is_active: z.boolean().optional(),
  metadata: z.record(z.unknown()).optional(),
});

function getCtx(c: { req: { header: (name: string) => string | undefined } }): RequestContext {
  const authorization = c.req.header('Authorization');
  return authorization ? { authorization } : {};
}

export const categoriesRoutes = new Hono();

// GET /api/categories
categoriesRoutes.get('/', async (c) => {
  const ctx = getCtx(c);
  const categories = await catalogClient.listCategories(ctx);
  return c.json({ success: true, data: categories });
});

// POST /api/categories
categoriesRoutes.post('/', async (c) => {
  const ctx = getCtx(c);
  const raw = await c.req.json();
  const parsed = createCategorySchema.safeParse(raw);
  if (!parsed.success) {
    return c.json(
      {
        success: false,
        message: 'Datos de categoría inválidos',
        errors: parsed.error.flatten().fieldErrors,
      },
      400,
    );
  }
  const created = await catalogClient.createCategory(parsed.data, ctx);
  return c.json({ success: true, data: created }, 201);
});

// DELETE /api/categories/:id
categoriesRoutes.delete('/:id', async (c) => {
  const ctx = getCtx(c);
  const id = c.req.param('id');
  await catalogClient.deleteCategory(id, ctx);
  return c.json({ success: true, message: 'Categoría eliminada' });
});

export const itemsRoutes = new Hono();

// POST /api/items/search
itemsRoutes.post('/search', async (c) => {
  const ctx = getCtx(c);
  const criteria = (await c.req.json().catch(() => ({}))) as Criteria;
  const result = await catalogClient.searchItems(criteria, ctx);
  return c.json(result);
});

// POST /api/items
itemsRoutes.post('/', async (c) => {
  const ctx = getCtx(c);
  const raw = await c.req.json();
  const parsed = createItemSchema.safeParse(raw);
  if (!parsed.success) {
    return c.json(
      {
        success: false,
        message: 'Datos de ítem de catálogo inválidos',
        errors: parsed.error.flatten().fieldErrors,
      },
      400,
    );
  }

  const created = await catalogClient.createItem(parsed.data as CreateCatalogItemCommand, ctx);
  return c.json({ success: true, data: created }, 201);
});

// PATCH /api/items/:id
itemsRoutes.patch('/:id', async (c) => {
  const ctx = getCtx(c);
  const id = c.req.param('id');
  const raw = await c.req.json();
  const parsed = updateItemSchema.safeParse(raw);
  if (!parsed.success) {
    return c.json(
      {
        success: false,
        message: 'Datos de actualización inválidos',
        errors: parsed.error.flatten().fieldErrors,
      },
      400,
    );
  }

  const data = parsed.data;
  let details: ItemDetails | undefined = data.details;

  if (!details && data.item_type) {
    if (data.item_type === 'SERVICE' && data.duration_minutes) {
      details = {
        item_type: 'SERVICE',
        duration_minutes: data.duration_minutes,
        buffer_minutes: data.buffer_minutes ?? 0,
      };
    } else if (data.item_type === 'PRODUCT') {
      details = {
        item_type: 'PRODUCT',
        unit_of_measure: data.unit_of_measure ?? null,
        track_inventory: data.track_inventory ?? false,
      };
    } else if (data.item_type === 'BUNDLE') {
      details = {
        item_type: 'BUNDLE',
        items: data.items ?? [],
      };
    }
  }

  const cmd: UpdateCatalogItemCommand = {
    category_id: data.category_id,
    sku: data.sku,
    name: data.name,
    description: data.description,
    base_price: data.base_price,
    currency: data.currency,
    tax_rate_bps: data.tax_rate_bps,
    tax_included: data.tax_included,
    details,
    is_active: data.is_active,
    metadata: data.metadata,
  };

  const updated = await catalogClient.updateItem(id, cmd, ctx);
  return c.json({ success: true, data: updated });
});

// DELETE /api/items/:id
itemsRoutes.delete('/:id', async (c) => {
  const ctx = getCtx(c);
  const id = c.req.param('id');
  await catalogClient.deleteItem(id, ctx);
  return c.json({ success: true, message: 'Ítem eliminado' });
});
