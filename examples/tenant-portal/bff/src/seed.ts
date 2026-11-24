import 'dotenv/config';
import { bookingsClient } from './clients/bookingsClient.js';
import { catalogClient } from './clients/catalogClient.js';
import { crmClient } from './clients/crmClient.js';
import { eventsClient } from './clients/eventsClient.js';
import { UpstreamApiError } from './clients/http.js';
import type { Calendar, CatalogItem, Category, Client } from './types/platform.js';

async function ensureCalendar(): Promise<Calendar> {
  const existing = await eventsClient.listCalendars();
  const found = existing.find((c) => c.name === 'Agenda Principal (Sin Solapamientos)');
  if (found) {
    console.log(`  ↺ Calendario existente reutilizado: ${found.name} (${found.id})`);
    return found;
  }

  const created = await eventsClient.createCalendar({
    name: 'Agenda Principal (Sin Solapamientos)',
    timezone: 'America/Santiago',
    allow_overlaps: false,
    working_hours: {
      monday: [{ start: '09:00', end: '18:00' }],
      tuesday: [{ start: '09:00', end: '18:00' }],
      wednesday: [{ start: '09:00', end: '18:00' }],
      thursday: [{ start: '09:00', end: '18:00' }],
      friday: [{ start: '09:00', end: '18:00' }],
      saturday: [{ start: '10:00', end: '14:00' }],
    },
  });
  console.log(`  ✓ Calendario creado: ${created.name} (${created.id})`);
  return created;
}

async function ensureCategories(): Promise<{ servicesCat: Category; productsCat: Category }> {
  const existing = await catalogClient.listCategories();

  const findOrCreate = async (
    name: string,
    slug: string,
    description: string,
    display_order: number,
  ): Promise<Category> => {
    const found = existing.find((c) => c.slug === slug);
    if (found) {
      console.log(`  ↺ Categoría existente: ${found.name} (${found.id})`);
      return found;
    }
    const created = await catalogClient.createCategory({
      name,
      slug,
      description,
      display_order,
      is_active: true,
    });
    console.log(`  ✓ Categoría creada: ${created.name} (${created.id})`);
    return created;
  };

  const servicesCat = await findOrCreate(
    'Servicios Profesionales',
    'servicios-profesionales',
    'Servicios agendables con duración fija',
    1,
  );
  const productsCat = await findOrCreate(
    'Productos de Cuidado',
    'productos-cuidado',
    'Productos físicos disponibles en tienda',
    2,
  );

  return { servicesCat, productsCat };
}

async function ensureCatalogItems(
  servicesCat: Category,
  productsCat: Category,
): Promise<CatalogItem[]> {
  const existingPage = await catalogClient.searchItems({
    pagination: { page: 1, limit: 100 },
  });
  const bySku = new Map(
    existingPage.data.filter((i) => i.sku).map((i) => [i.sku!, i]),
  );

  const specs = [
    {
      sku: 'SRV-DIAG-30',
      name: 'Evaluación & Diagnóstico Inicial',
      description: 'Sesión diagnóstica personalizada de 30 minutos.',
      category_id: servicesCat.id,
      base_price: 25000,
      currency: 'CLP',
      tax_rate_bps: 1900,
      tax_included: true,
      item_type: 'SERVICE' as const,
      duration_minutes: 30,
      buffer_minutes: 10,
    },
    {
      sku: 'SRV-TRAT-60',
      name: 'Sesión de Tratamiento Integral',
      description: 'Sesión completa de 60 minutos con seguimiento.',
      category_id: servicesCat.id,
      base_price: 48000,
      currency: 'CLP',
      tax_rate_bps: 1900,
      tax_included: true,
      item_type: 'SERVICE' as const,
      duration_minutes: 60,
      buffer_minutes: 15,
    },
    {
      sku: 'PRD-KIT-01',
      name: 'Kit de Cuidado Diario (250ml)',
      description: 'Producto físico complementario para uso en casa.',
      category_id: productsCat.id,
      base_price: 18500,
      currency: 'CLP',
      tax_rate_bps: 1900,
      tax_included: true,
      item_type: 'PRODUCT' as const,
      unit_of_measure: 'UNIT',
      track_inventory: true,
    },
  ];

  const items: CatalogItem[] = [];
  for (const spec of specs) {
    const found = bySku.get(spec.sku);
    if (found) {
      console.log(`  ↺ Ítem existente: ${found.name} (${found.sku})`);
      items.push(found);
      continue;
    }
    const created = await catalogClient.createItem(spec);
    console.log(`  ✓ Ítem creado: ${created.name} (${created.item_type} · ${created.base_price} ${created.currency})`);
    items.push(created);
  }

  return items;
}

async function ensureClients(): Promise<Client[]> {
  const existingPage = await crmClient.searchClients({
    pagination: { page: 1, limit: 100 },
  });
  const byEmail = new Map(
    existingPage.data.filter((c) => c.email).map((c) => [c.email!.toLowerCase(), c]),
  );

  const specs = [
    {
      full_name: 'Camila Rojas Valdés',
      email: 'camila.rojas@ejemplo.cl',
      phone: '+56 9 8123 4567',
      tax_id: '18.456.789-2',
      address: 'Av. Providencia 1234, Santiago',
      notes: 'Cliente frecuente, prefiere horarios de mañana.',
      metadata: { segment: 'VIP', preferred_channel: 'WhatsApp' },
    },
    {
      full_name: 'Matías Fernández Soto',
      email: 'matias.fernandez@ejemplo.cl',
      phone: '+56 9 7654 3210',
      tax_id: '16.987.654-K',
      address: 'Las Condes 4500, Santiago',
      notes: 'Primera visita referida por redes sociales.',
      metadata: { segment: 'Regular', referral: 'Instagram' },
    },
    {
      full_name: 'Valentina Muñoz Paredes',
      email: 'valentina.munoz@ejemplo.cl',
      phone: '+56 9 6543 2109',
      tax_id: '19.112.233-4',
      address: 'Ñuñoa, Santiago',
      notes: 'Solicita factura electrónica para empresa.',
      metadata: { segment: 'Corporate', billing_type: 'Factura' },
    },
  ];

  const clients: Client[] = [];
  for (const spec of specs) {
    const found = byEmail.get(spec.email.toLowerCase());
    if (found) {
      console.log(`  ↺ Cliente existente: ${found.full_name} (${found.email})`);
      clients.push(found);
      continue;
    }
    const created = await crmClient.createClient(spec);
    console.log(`  ✓ Cliente creado: ${created.full_name} (${created.tax_id})`);
    clients.push(created);
  }

  return clients;
}

async function seedBookings(
  calendar: Calendar,
  clients: Client[],
  items: CatalogItem[],
): Promise<void> {
  // Schedule 2 realistic bookings for tomorrow at 10:00 and 14:00 UTC (adjacent/non-overlapping)
  const tomorrow = new Date();
  tomorrow.setUTCDate(tomorrow.getUTCDate() + 1);
  tomorrow.setUTCMinutes(0, 0, 0);

  const service1 = items[0]; // 30 min
  const service2 = items[1]; // 60 min

  const bookingSpecs = [
    {
      client: clients[0],
      selectedItems: [service1],
      hourUtc: 14, // 10:00-11:00 America/Santiago approx
      notes: 'Reserva inicial generada por seed.ts',
    },
    {
      client: clients[1],
      selectedItems: [service1, service2],
      hourUtc: 16, // 12:00-13:00 America/Santiago approx
      notes: 'Sesión combinada (diagnóstico + tratamiento)',
    },
  ];

  for (const spec of bookingSpecs) {
    const start = new Date(tomorrow);
    start.setUTCHours(spec.hourUtc, 0, 0, 0);

    const totalDurationMinutes =
      spec.selectedItems.reduce(
        (sum, item) =>
          sum +
          (item.item_type === 'SERVICE'
            ? item.duration_minutes + (item.buffer_minutes ?? 0)
            : 0),
        0,
      ) || 30;
    const currency = spec.selectedItems[0]?.currency ?? 'CLP';
    const end = new Date(start.getTime() + totalDurationMinutes * 60 * 1000);

    try {
      const event = await eventsClient.createEvent({
        calendar_id: calendar.id,
        title: `${spec.client.full_name} — ${spec.selectedItems.map((i) => i.name).join(' + ')}`,
        description: spec.notes,
        starts_at: start.toISOString(),
        ends_at: end.toISOString(),
        timezone: calendar.timezone || 'UTC',
        status: 'CONFIRMED',
        external_resource_id: spec.client.id,
        metadata: {
          source: 'seed.ts',
          client_id: spec.client.id,
        },
      });

      try {
        const booking = await bookingsClient.createBooking({
          client_id: spec.client.id,
          calendar_id: calendar.id,
          event_id: event.id,
          starts_at: start.toISOString(),
          ends_at: end.toISOString(),
          timezone: calendar.timezone || 'UTC',
          status: 'SCHEDULED',
          currency,
          notes: spec.notes,
          metadata: { source: 'seed.ts' },
          items: spec.selectedItems.map((item) => ({
            catalog_item_id: item.id,
            variant_id: null,
            name_snapshot: item.name,
            sku_snapshot: item.sku ?? null,
            quantity: 1,
            unit_price_snapshot: item.base_price,
            tax_rate_bps: item.tax_rate_bps,
            tax_included: item.tax_included,
            duration_minutes_snapshot: item.item_type === 'SERVICE' ? item.duration_minutes : 0,
            buffer_minutes_snapshot: item.item_type === 'SERVICE' ? (item.buffer_minutes ?? 0) : 0,
          })),
        });
        console.log(
          `  ✓ Reserva creada: ${booking.booking_code} ("${event.title}", ${start.toISOString()} -> ${end.toISOString()})`,
        );
      } catch (bookingErr) {
        await eventsClient.deleteEvent(event.id).catch(() => undefined);
        throw bookingErr;
      }
    } catch (err) {
      if (err instanceof UpstreamApiError && err.status === 409) {
        console.log(
          `  ↺ Reserva ya existente en el bloque ${start.toISOString()} (409 Conflict esperado, omitiendo).`,
        );
      } else {
        throw err;
      }
    }
  }
}

async function main() {
  console.log('🌱 Iniciando seed de datos de prueba en ikedadev-platform...\n');

  console.log('1/5 Calendario en events-api...');
  const calendar = await ensureCalendar();

  console.log('\n2/5 Categorías en catalog-api...');
  const { servicesCat, productsCat } = await ensureCategories();

  console.log('\n3/5 Ítems de catálogo en catalog-api...');
  const items = await ensureCatalogItems(servicesCat, productsCat);

  console.log('\n4/5 Clientes en crm-api...');
  const clients = await ensureClients();

  console.log('\n5/5 Reservas en events-api...');
  await seedBookings(calendar, clients, items);

  console.log('\n✅ Seed completado con éxito. Ya puedes abrir el Dashboard Web!');
}

main().catch((err) => {
  console.error('\n❌ Error ejecutando seed:', err);
  process.exit(1);
});
